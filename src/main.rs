mod app;
mod diff;
mod git;
mod review;
mod ui;

use anyhow::Result;
use app::{App, Mode};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;
use std::io::stderr;
use std::time::Duration;

#[cfg(not(tarpaulin_include))]
fn main() -> Result<()> {
    use std::panic::{self, AssertUnwindSafe};

    let files = git::get_changed_files()?;

    execute!(stderr(), EnableMouseCapture)?;
    let mut terminal = ratatui::init();
    let mut app = App::new(files);

    if !app.files.is_empty() {
        app.select_file(0);
    }

    let result = panic::catch_unwind(AssertUnwindSafe(|| run(&mut terminal, &mut app)));

    ratatui::restore();
    execute!(stderr(), DisableMouseCapture)?;

    match result {
        Ok(inner) => inner,
        Err(panic_payload) => {
            let msg = panic_payload
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            anyhow::bail!("panic: {}", msg);
        }
    }
}

#[cfg(not(tarpaulin_include))]
fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            let size = terminal.size()?;
            let frame_rect = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            handle_event(app, ev, frame_rect);
            if app.should_quit {
                return Ok(());
            }
        }
    }
}

pub(crate) fn handle_event(app: &mut App, event: Event, frame_size: ratatui::layout::Rect) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.status_message = None;
            match app.mode {
                Mode::Normal => handle_normal_key(app, key.code),
                Mode::Commenting => handle_commenting_key(app, key),
                Mode::Summary => handle_summary_key(app, key),
            }
        }
        Event::Mouse(mouse) if app.mode == Mode::Normal => {
            handle_mouse(app, mouse, frame_size);
        }
        _ => {}
    }
}

fn handle_normal_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('q') => app.should_quit = true,
        KeyCode::Char('s') => {
            let summary = app.summary.clone();
            app.start_input(&summary);
            app.mode = Mode::Summary;
        }
        KeyCode::Char('S') => submit_review(app),
        KeyCode::Up | KeyCode::Char('k') => {
            app.diff_scroll = app.diff_scroll.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.diff_scroll += 1;
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.diff_hscroll = app.diff_hscroll.saturating_sub(4);
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.diff_hscroll += 4;
        }
        KeyCode::Tab => {
            let next = app.file_list_state.selected().map_or(0, |i| {
                if i + 1 < app.files.len() {
                    i + 1
                } else {
                    0
                }
            });
            app.select_file(next);
        }
        KeyCode::BackTab => {
            let prev = app.file_list_state.selected().map_or(0, |i| {
                if i == 0 {
                    app.files.len().saturating_sub(1)
                } else {
                    i - 1
                }
            });
            app.select_file(prev);
        }
        KeyCode::Char('r') => {
            if let Ok(files) = git::get_changed_files() {
                refresh_file_list(app, files);
            }
        }
        _ => {}
    }
}

fn handle_commenting_key(app: &mut App, key: KeyEvent) {
    app.cursor_blink_start = std::time::Instant::now();
    match (key.code, key.modifiers) {
        (KeyCode::Enter, m) if m.contains(KeyModifiers::ALT) => {
            if let Some(textarea) = &mut app.textarea {
                textarea.insert_newline();
            }
        }
        (KeyCode::Enter, _) => app.submit_comment(),
        (KeyCode::Esc, _) => {
            app.clear_input();
            app.commenting_line = None;
            app.mode = Mode::Normal;
        }
        _ => {
            if let Some(textarea) = &mut app.textarea {
                textarea.input(key);
            }
        }
    }
}

fn handle_summary_key(app: &mut App, key: KeyEvent) {
    app.cursor_blink_start = std::time::Instant::now();
    match (key.code, key.modifiers) {
        (KeyCode::Enter, m) if m.contains(KeyModifiers::ALT) => {
            if let Some(textarea) = &mut app.textarea {
                textarea.insert_newline();
            }
        }
        (KeyCode::Enter, _) => app.submit_summary(),
        (KeyCode::Esc, _) => {
            app.clear_input();
            app.mode = Mode::Normal;
        }
        _ => {
            if let Some(textarea) = &mut app.textarea {
                textarea.input(key);
            }
        }
    }
}

fn handle_mouse(
    app: &mut App,
    mouse: ratatui::crossterm::event::MouseEvent,
    frame_size: ratatui::layout::Rect,
) {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            handle_mouse_click(app, mouse.column, mouse.row, frame_size);
        }
        MouseEventKind::ScrollUp => {
            app.diff_scroll = app.diff_scroll.saturating_sub(3);
        }
        MouseEventKind::ScrollDown => {
            app.diff_scroll += 3;
        }
        MouseEventKind::ScrollLeft => {
            app.diff_hscroll = app.diff_hscroll.saturating_sub(4);
        }
        MouseEventKind::ScrollRight => {
            app.diff_hscroll += 4;
        }
        _ => {}
    }
}

fn handle_mouse_click(app: &mut App, col: u16, row: u16, frame_size: ratatui::layout::Rect) {
    // Check if click is in file list
    let file_area = ui::file_list_area(frame_size);
    if col >= file_area.x
        && col < file_area.x + file_area.width
        && row > file_area.y
        && row < file_area.y + file_area.height - 1
    {
        let index = (row - file_area.y - 1) as usize;
        if index < app.files.len() {
            app.select_file(index);
        }
        return;
    }

    // Check if click is in diff area
    let diff_inner = ui::diff_area(frame_size);
    if col >= diff_inner.x
        && col < diff_inner.x + diff_inner.width
        && row >= diff_inner.y
        && row < diff_inner.y + diff_inner.height
    {
        let clicked_row = (row - diff_inner.y) as usize + app.diff_scroll;
        if let Some(line_idx) = map_row_to_diff_line(app, clicked_row) {
            app.commenting_line = Some(line_idx);
            app.mode = Mode::Commenting;
            app.start_input("");
        }
    }
}

/// Calculate the number of visual lines a comment card occupies.
/// Matches CommentCard::to_lines(): 1 top border + content lines + 1 bottom border.
fn comment_card_height(text: &str) -> usize {
    let content_lines = if text.is_empty() {
        1
    } else {
        text.lines().count()
    };
    2 + content_lines
}

pub(crate) fn map_row_to_diff_line(app: &App, target_row: usize) -> Option<usize> {
    let diff = app.current_diff.as_ref()?;
    let file_comments = app.current_file.as_ref().and_then(|f| app.comments.get(f));

    let all_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();
    let mut visual_row = 0;

    for (idx, _line) in all_lines.iter().enumerate() {
        if visual_row == target_row {
            return Some(idx);
        }
        visual_row += 1;
        if let Some(comments) = file_comments {
            for c in comments.iter().filter(|c| c.line_index == idx) {
                visual_row += comment_card_height(&c.text);
            }
        }
    }
    None
}

pub(crate) fn refresh_file_list(app: &mut App, files: Vec<git::ChangedFile>) {
    app.files = files;
    if !app.files.is_empty() {
        app.select_file(0);
    } else {
        app.current_diff = None;
        app.current_file = None;
    }
    app.status_message = Some("Refreshed file list".to_string());
}

#[cfg(not(tarpaulin_include))]
fn submit_review(app: &mut App) {
    let output = review::format_review(app);
    if output.is_empty() {
        return;
    }
    if review::copy_to_clipboard(&output).is_ok() {
        app.status_message = Some("Review copied to clipboard!".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode, ReviewComment};
    use crate::diff::{DiffLine, FileDiff, Hunk, LineType};
    use crate::git::{ChangeType, ChangedFile};
    use ratatui::crossterm::event::{
        Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton,
        MouseEvent, MouseEventKind,
    };
    use ratatui::layout::Rect;

    fn key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn ctrl_key_event(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn mouse_click(col: u16, row: u16) -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: col,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn mouse_scroll_up() -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn mouse_scroll_down() -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn frame_size() -> Rect {
        Rect::new(0, 0, 80, 24)
    }

    fn make_test_app() -> App {
        let files = vec![
            ChangedFile {
                path: "file1.rs".to_string(),
                change_type: ChangeType::Modified,
                additions: 5,
                deletions: 3,
            },
            ChangedFile {
                path: "file2.rs".to_string(),
                change_type: ChangeType::Added,
                additions: 10,
                deletions: 0,
            },
        ];
        let mut app = App::new(files);
        let diff = FileDiff {
            hunks: vec![Hunk {
                lines: vec![
                    DiffLine {
                        line_type: LineType::HunkHeader,
                        content: "@@ -1,3 +1,4 @@".to_string(),
                        old_line_no: None,
                        new_line_no: None,
                    },
                    DiffLine {
                        line_type: LineType::Context,
                        content: "line 1".to_string(),
                        old_line_no: Some(1),
                        new_line_no: Some(1),
                    },
                    DiffLine {
                        line_type: LineType::Deletion,
                        content: "old".to_string(),
                        old_line_no: Some(2),
                        new_line_no: None,
                    },
                    DiffLine {
                        line_type: LineType::Addition,
                        content: "new".to_string(),
                        old_line_no: None,
                        new_line_no: Some(2),
                    },
                ],
            }],
        };
        app.select_file_with_diff(0, Some(diff));
        app
    }

    // ── Normal mode: 'q' sets should_quit ────────────────────────────

    #[test]
    fn normal_q_sets_should_quit() {
        let mut app = make_test_app();
        handle_event(&mut app, key_event(KeyCode::Char('q')), frame_size());
        assert!(app.should_quit);
    }

    // ── Normal mode: 's' switches to Summary mode ────────────────────

    #[test]
    fn normal_s_switches_to_summary_mode() {
        let mut app = make_test_app();
        app.summary = "existing summary".to_string();
        handle_event(&mut app, key_event(KeyCode::Char('s')), frame_size());
        assert_eq!(app.mode, Mode::Summary);
        assert_eq!(app.input_text(), "existing summary");
    }

    // ── Normal mode: j/Down increases diff_scroll ────────────────────

    #[test]
    fn normal_j_increases_diff_scroll() {
        let mut app = make_test_app();
        assert_eq!(app.diff_scroll, 0);
        handle_event(&mut app, key_event(KeyCode::Char('j')), frame_size());
        assert_eq!(app.diff_scroll, 1);
    }

    #[test]
    fn normal_down_increases_diff_scroll() {
        let mut app = make_test_app();
        handle_event(&mut app, key_event(KeyCode::Down), frame_size());
        assert_eq!(app.diff_scroll, 1);
    }

    // ── Normal mode: k/Up decreases diff_scroll (saturating) ─────────

    #[test]
    fn normal_k_decreases_diff_scroll() {
        let mut app = make_test_app();
        app.diff_scroll = 5;
        handle_event(&mut app, key_event(KeyCode::Char('k')), frame_size());
        assert_eq!(app.diff_scroll, 4);
    }

    #[test]
    fn normal_up_decreases_diff_scroll() {
        let mut app = make_test_app();
        app.diff_scroll = 3;
        handle_event(&mut app, key_event(KeyCode::Up), frame_size());
        assert_eq!(app.diff_scroll, 2);
    }

    #[test]
    fn normal_k_saturates_at_zero() {
        let mut app = make_test_app();
        app.diff_scroll = 0;
        handle_event(&mut app, key_event(KeyCode::Char('k')), frame_size());
        assert_eq!(app.diff_scroll, 0);
    }

    // ── Normal mode: Tab selects next file (wrapping) ────────────────

    #[test]
    fn normal_tab_selects_next_file() {
        let mut app = make_test_app();
        assert_eq!(app.file_list_state.selected(), Some(0));
        handle_event(&mut app, key_event(KeyCode::Tab), frame_size());
        assert_eq!(app.file_list_state.selected(), Some(1));
    }

    #[test]
    fn normal_tab_wraps_to_first() {
        let mut app = make_test_app();
        // Select second file (index 1)
        app.select_file_with_diff(1, None);
        assert_eq!(app.file_list_state.selected(), Some(1));
        handle_event(&mut app, key_event(KeyCode::Tab), frame_size());
        assert_eq!(app.file_list_state.selected(), Some(0));
    }

    // ── Normal mode: BackTab selects previous file (wrapping) ────────

    #[test]
    fn normal_backtab_selects_previous_file() {
        let mut app = make_test_app();
        app.select_file_with_diff(1, None);
        handle_event(&mut app, key_event(KeyCode::BackTab), frame_size());
        assert_eq!(app.file_list_state.selected(), Some(0));
    }

    #[test]
    fn normal_backtab_wraps_to_last() {
        let mut app = make_test_app();
        assert_eq!(app.file_list_state.selected(), Some(0));
        handle_event(&mut app, key_event(KeyCode::BackTab), frame_size());
        assert_eq!(app.file_list_state.selected(), Some(1));
    }

    // ── Normal mode: any key clears status_message ───────────────────

    #[test]
    fn normal_any_key_clears_status_message() {
        let mut app = make_test_app();
        app.status_message = Some("some status".to_string());
        handle_event(&mut app, key_event(KeyCode::Char('j')), frame_size());
        assert!(app.status_message.is_none());
    }

    // ── Normal mode: unknown key does nothing ────────────────────────

    #[test]
    fn normal_unknown_key_does_nothing() {
        let mut app = make_test_app();
        let scroll_before = app.diff_scroll;
        let mode_before = app.mode.clone();
        handle_event(&mut app, key_event(KeyCode::Char('z')), frame_size());
        assert_eq!(app.diff_scroll, scroll_before);
        assert_eq!(app.mode, mode_before);
        assert!(!app.should_quit);
    }

    // ── Commenting mode: Char appends to input_buffer ────────────────

    #[test]
    fn commenting_char_appends_to_buffer() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);
        app.start_input("hel");
        handle_event(&mut app, key_event(KeyCode::Char('l')), frame_size());
        assert_eq!(app.input_text(), "hell");
    }

    // ── Commenting mode: Backspace removes last char ─────────────────

    #[test]
    fn commenting_backspace_removes_last_char() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);
        app.start_input("hello");
        handle_event(&mut app, key_event(KeyCode::Backspace), frame_size());
        assert_eq!(app.input_text(), "hell");
    }

    // ── Commenting mode: Esc cancels ─────────────────────────────────

    #[test]
    fn commenting_esc_cancels() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(2);
        app.start_input("some partial comment");
        handle_event(&mut app, key_event(KeyCode::Esc), frame_size());
        assert!(app.textarea.is_none());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.commenting_line.is_none());
    }

    // ── Commenting mode: Enter submits comment ───────────────────────

    #[test]
    fn commenting_enter_submits_comment() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(1);
        app.start_input("nice line");
        handle_event(&mut app, key_event(KeyCode::Enter), frame_size());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.textarea.is_none());
        assert!(app.commenting_line.is_none());
        let comments = app.comments.get("file1.rs").unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].line_index, 1);
        assert_eq!(comments[0].text, "nice line");
    }

    // ── Summary mode: Char appends to buffer ─────────────────────────

    #[test]
    fn summary_char_appends_to_buffer() {
        let mut app = make_test_app();
        app.start_input("sum");
        app.mode = Mode::Summary;
        handle_event(&mut app, key_event(KeyCode::Char('m')), frame_size());
        assert_eq!(app.input_text(), "summ");
    }

    // ── Summary mode: Enter saves summary ────────────────────────────

    #[test]
    fn summary_enter_saves_summary() {
        let mut app = make_test_app();
        app.start_input("my review summary");
        app.mode = Mode::Summary;
        handle_event(&mut app, key_event(KeyCode::Enter), frame_size());
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.summary, "my review summary");
        assert!(app.textarea.is_none());
    }

    // ── Summary mode: Esc cancels ────────────────────────────────────

    #[test]
    fn summary_esc_cancels() {
        let mut app = make_test_app();
        app.summary = "old summary".to_string();
        app.start_input("new draft");
        app.mode = Mode::Summary;
        handle_event(&mut app, key_event(KeyCode::Esc), frame_size());
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.textarea.is_none());
        // Original summary should be unchanged
        assert_eq!(app.summary, "old summary");
    }

    // ── Mouse: click in file list selects file ───────────────────────

    #[test]
    fn mouse_click_file_list_selects_file() {
        let mut app = make_test_app();
        let file_area = ui::file_list_area(frame_size());
        // Click on the second file (row = file_area.y + 1 for border + 1 for second item)
        let click_col = file_area.x + 1;
        let click_row = file_area.y + 2; // border row + first item row -> second item
        handle_event(&mut app, mouse_click(click_col, click_row), frame_size());
        assert_eq!(app.file_list_state.selected(), Some(1));
    }

    // ── Mouse: click in diff area starts commenting ──────────────────

    #[test]
    fn mouse_click_diff_area_starts_commenting() {
        let mut app = make_test_app();
        let diff_inner = ui::diff_area(frame_size());
        // Click on the first visible row of the diff
        let click_col = diff_inner.x + 1;
        let click_row = diff_inner.y;
        handle_event(&mut app, mouse_click(click_col, click_row), frame_size());
        assert_eq!(app.mode, Mode::Commenting);
        assert!(app.commenting_line.is_some());
        assert!(app.textarea.is_some());
        assert_eq!(app.input_text(), "");
    }

    // ── Mouse: events ignored when not in Normal mode ────────────────

    #[test]
    fn mouse_events_ignored_in_commenting_mode() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);
        let diff_inner = ui::diff_area(frame_size());
        let click_col = diff_inner.x + 1;
        let click_row = diff_inner.y;
        handle_event(&mut app, mouse_click(click_col, click_row), frame_size());
        // Mode should still be Commenting, not changed
        assert_eq!(app.mode, Mode::Commenting);
    }

    #[test]
    fn mouse_events_ignored_in_summary_mode() {
        let mut app = make_test_app();
        app.mode = Mode::Summary;
        let scroll_before = app.diff_scroll;
        handle_event(&mut app, mouse_scroll_down(), frame_size());
        assert_eq!(app.diff_scroll, scroll_before);
        assert_eq!(app.mode, Mode::Summary);
    }

    // ── Mouse: scroll up decreases diff_scroll by 3 ──────────────────

    #[test]
    fn mouse_scroll_up_decreases_diff_scroll() {
        let mut app = make_test_app();
        app.diff_scroll = 10;
        handle_event(&mut app, mouse_scroll_up(), frame_size());
        assert_eq!(app.diff_scroll, 7);
    }

    #[test]
    fn mouse_scroll_up_saturates_at_zero() {
        let mut app = make_test_app();
        app.diff_scroll = 1;
        handle_event(&mut app, mouse_scroll_up(), frame_size());
        assert_eq!(app.diff_scroll, 0);
    }

    // ── Mouse: scroll down increases diff_scroll by 3 ────────────────

    #[test]
    fn mouse_scroll_down_increases_diff_scroll() {
        let mut app = make_test_app();
        app.diff_scroll = 0;
        handle_event(&mut app, mouse_scroll_down(), frame_size());
        assert_eq!(app.diff_scroll, 3);
    }

    // ── map_row_to_diff_line: no diff returns None ───────────────────

    #[test]
    fn map_row_no_diff_returns_none() {
        let mut app = make_test_app();
        app.current_diff = None;
        assert_eq!(map_row_to_diff_line(&app, 0), None);
    }

    // ── map_row_to_diff_line: row 0 returns Some(0) ──────────────────

    #[test]
    fn map_row_zero_returns_first_line() {
        let app = make_test_app();
        assert_eq!(map_row_to_diff_line(&app, 0), Some(0));
    }

    // ── map_row_to_diff_line: row beyond diff lines returns None ──────

    #[test]
    fn map_row_beyond_diff_returns_none() {
        let app = make_test_app();
        // The diff has 4 lines (indices 0..3), so row 4 is beyond
        assert_eq!(map_row_to_diff_line(&app, 4), None);
        assert_eq!(map_row_to_diff_line(&app, 100), None);
    }

    // ── map_row_to_diff_line: with comments, rows are offset ─────────

    #[test]
    fn map_row_with_comments_offset() {
        let mut app = make_test_app();
        // Add a comment on diff line 0
        app.comments.insert(
            "file1.rs".to_string(),
            vec![ReviewComment {
                line_index: 0,
                text: "a comment".to_string(),
            }],
        );
        // Row 0 -> diff line 0
        assert_eq!(map_row_to_diff_line(&app, 0), Some(0));
        // Comment card takes 3 rows (border + content + border), so rows 1-3 are card
        // Row 4 -> diff line 1
        assert_eq!(map_row_to_diff_line(&app, 4), Some(1));
        // Row 5 -> diff line 2
        assert_eq!(map_row_to_diff_line(&app, 5), Some(2));
        // Row 6 -> diff line 3
        assert_eq!(map_row_to_diff_line(&app, 6), Some(3));
        // Row 7 is beyond (4 diff lines + 3 card rows = 7 visual rows total)
        assert_eq!(map_row_to_diff_line(&app, 7), None);
    }

    // ── Normal mode: 'S' calls submit_review (excluded from coverage) ──

    #[test]
    fn normal_shift_s_calls_submit_review() {
        let mut app = make_test_app();
        // submit_review is excluded from tarpaulin but the match arm is covered
        handle_event(&mut app, key_event(KeyCode::Char('S')), frame_size());
        // Should not panic; submit_review calls git which may fail, but that's fine
    }

    // ── Normal mode: 'r' refreshes file list ───────────────────────────

    #[test]
    fn normal_r_refreshes_files() {
        let mut app = make_test_app();
        // This calls git::get_changed_files() which may succeed or fail
        // depending on the git state. Either way, the handler should not panic.
        handle_event(&mut app, key_event(KeyCode::Char('r')), frame_size());
        // If git succeeded, status_message should be set
        // If git failed, status_message should remain None
        // We just verify no panic occurred
    }

    // ── refresh_file_list: with files selects first ─────────────────────

    #[test]
    fn refresh_file_list_with_files() {
        let mut app = make_test_app();
        let new_files = vec![ChangedFile {
            path: "new.rs".to_string(),
            change_type: ChangeType::Added,
            additions: 1,
            deletions: 0,
        }];
        refresh_file_list(&mut app, new_files);
        assert_eq!(app.files.len(), 1);
        assert_eq!(app.files[0].path, "new.rs");
        assert_eq!(app.status_message.as_deref(), Some("Refreshed file list"));
    }

    #[test]
    fn refresh_file_list_empty_clears_diff() {
        let mut app = make_test_app();
        assert!(app.current_diff.is_some());
        assert!(app.current_file.is_some());
        refresh_file_list(&mut app, vec![]);
        assert!(app.files.is_empty());
        assert!(app.current_diff.is_none());
        assert!(app.current_file.is_none());
        assert_eq!(app.status_message.as_deref(), Some("Refreshed file list"));
    }

    // ── Commenting mode: unknown key does nothing ─────────────────────

    #[test]
    fn commenting_unknown_key_forwarded_to_textarea() {
        let mut app = make_test_app();
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);
        app.start_input("text");
        handle_event(&mut app, key_event(KeyCode::F(1)), frame_size());
        // TextArea handles or ignores keys — mode stays Commenting
        assert_eq!(app.mode, Mode::Commenting);
    }

    // ── Summary mode: Backspace removes last char ─────────────────────

    #[test]
    fn summary_backspace_removes_last_char() {
        let mut app = make_test_app();
        app.start_input("hello");
        app.mode = Mode::Summary;
        handle_event(&mut app, key_event(KeyCode::Backspace), frame_size());
        assert_eq!(app.input_text(), "hell");
    }

    // ── Summary mode: unknown key forwarded to textarea ───────────────

    #[test]
    fn summary_unknown_key_forwarded_to_textarea() {
        let mut app = make_test_app();
        app.start_input("text");
        app.mode = Mode::Summary;
        handle_event(&mut app, key_event(KeyCode::F(1)), frame_size());
        // TextArea handles or ignores keys — mode stays Summary
        assert_eq!(app.mode, Mode::Summary);
    }

    // ── Mouse: click outside both file list and diff area ─────────────

    #[test]
    fn mouse_click_outside_both_areas_does_nothing() {
        let mut app = make_test_app();
        let mode_before = app.mode.clone();
        // Click in the status bar area (very bottom)
        handle_event(&mut app, mouse_click(5, 23), frame_size());
        assert_eq!(app.mode, mode_before);
    }

    // ── Mouse: non-click non-scroll events ignored ────────────────────

    #[test]
    fn mouse_move_event_does_nothing() {
        let mut app = make_test_app();
        let scroll_before = app.diff_scroll;
        let ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        });
        handle_event(&mut app, ev, frame_size());
        assert_eq!(app.diff_scroll, scroll_before);
    }

    // ── Non-key, non-mouse events are ignored ─────────────────────────

    #[test]
    fn resize_event_does_nothing() {
        let mut app = make_test_app();
        let ev = Event::Resize(100, 50);
        handle_event(&mut app, ev, frame_size());
        assert!(!app.should_quit);
    }

    #[test]
    fn map_row_with_multiple_comments_on_same_line() {
        let mut app = make_test_app();
        // Two comments on diff line 1
        app.comments.insert(
            "file1.rs".to_string(),
            vec![
                ReviewComment {
                    line_index: 1,
                    text: "first comment".to_string(),
                },
                ReviewComment {
                    line_index: 1,
                    text: "second comment".to_string(),
                },
            ],
        );
        // Row 0 -> diff line 0
        assert_eq!(map_row_to_diff_line(&app, 0), Some(0));
        // Row 1 -> diff line 1
        assert_eq!(map_row_to_diff_line(&app, 1), Some(1));
        // Two cards at 3 rows each = 6 visual rows for comments (rows 2-7)
        // Row 8 -> diff line 2
        assert_eq!(map_row_to_diff_line(&app, 8), Some(2));
    }
}
