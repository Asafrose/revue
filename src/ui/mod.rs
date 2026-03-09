mod comment_card;
mod commit_list;
mod diff;
mod file_list;
mod status_bar;

use crate::app::App;
use commit_list::CommitListWidget;
use diff::DiffWidget;
use file_list::FileListWidget;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;
use status_bar::StatusBarWidget;

/// Default width of the sidebar (file list + commit list)
pub const DEFAULT_SIDEBAR_WIDTH: u16 = 28;
/// Minimum sidebar width
const MIN_SIDEBAR_WIDTH: u16 = 16;
/// Minimum diff pane width
const MIN_DIFF_WIDTH: u16 = 20;
/// Height of the commit list pane
const COMMIT_LIST_HEIGHT: u16 = 10;

pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, status_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame.area());

    let sidebar_w = app.sidebar_width;
    let [diff_area, sidebar_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(sidebar_w)]).areas(main_area);

    let [files_area, commits_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(COMMIT_LIST_HEIGHT)])
            .areas(sidebar_area);

    frame.render_stateful_widget(FileListWidget, files_area, app);
    frame.render_stateful_widget(CommitListWidget, commits_area, app);
    frame.render_stateful_widget(DiffWidget, diff_area, app);
    frame.render_widget(StatusBarWidget::new(app), status_area);
}

/// Shorten a path to fit the sidebar. Shows last dir + filename when possible.
pub(crate) fn short_path(path: &str) -> &str {
    if path.len() <= 18 {
        return path;
    }
    // Try to show "dir/file.ext"
    let mut parts = path.rsplitn(3, '/');
    let file = parts.next().unwrap_or(path);
    if let Some(dir) = parts.next() {
        // Find where "dir/file" starts in the original path
        let suffix_len = dir.len() + 1 + file.len();
        if suffix_len <= 18 {
            return &path[path.len() - suffix_len..];
        }
    }
    // Fall back to just filename
    path.rsplit('/').next().unwrap_or(path)
}

/// Compute sidebar layout areas.
fn sidebar_areas(frame_area: Rect, sidebar_width: u16) -> (Rect, Rect) {
    let [main_area, _status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame_area);
    let [_diff, sidebar] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(sidebar_width)])
            .areas(main_area);
    let [files, commits] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(COMMIT_LIST_HEIGHT)])
            .areas(sidebar);
    (files, commits)
}

/// Returns the area occupied by the file list panel.
pub fn file_list_area(frame_area: Rect, sidebar_width: u16) -> Rect {
    sidebar_areas(frame_area, sidebar_width).0
}

/// Returns the area occupied by the commit list panel.
pub fn commit_list_area(frame_area: Rect, sidebar_width: u16) -> Rect {
    sidebar_areas(frame_area, sidebar_width).1
}

/// Returns the inner area of the diff panel (inside borders).
pub fn diff_area(frame_area: Rect, sidebar_width: u16) -> Rect {
    let [main_area, _status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame_area);

    let [diff, _sidebar] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(sidebar_width)])
            .areas(main_area);

    Rect {
        x: diff.x + 1,
        y: diff.y + 1,
        width: diff.width.saturating_sub(2),
        height: diff.height.saturating_sub(2),
    }
}

/// Returns the column of the sidebar border (for drag detection).
pub fn sidebar_border_col(frame_area: Rect, sidebar_width: u16) -> u16 {
    let [main_area, _status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame_area);
    let [diff, _sidebar] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(sidebar_width)])
            .areas(main_area);
    diff.x + diff.width
}

/// Clamp sidebar width to valid range for a given frame width.
pub fn clamp_sidebar_width(width: u16, frame_width: u16) -> u16 {
    let max_width = frame_width.saturating_sub(MIN_DIFF_WIDTH);
    width.clamp(MIN_SIDEBAR_WIDTH, max_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode};
    use crate::diff::{DiffLine, FileDiff, Hunk, LineType};
    use crate::git::{ChangeType, ChangedFile};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn buffer_contains(buffer: &Buffer, text: &str) -> bool {
        let content: String = (0..buffer.area.height)
            .flat_map(|y| {
                let mut line: String = (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol().to_string())
                    .collect();
                line.push('\n');
                line.chars().collect::<Vec<_>>()
            })
            .collect();
        content.contains(text)
    }

    fn make_test_file(path: &str, change_type: ChangeType) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            change_type,
            additions: 5,
            deletions: 3,
        }
    }

    fn make_test_diff() -> FileDiff {
        FileDiff {
            hunks: vec![Hunk {
                lines: vec![
                    DiffLine {
                        line_type: LineType::Context,
                        content: "unchanged".to_string(),
                        old_line_no: Some(1),
                        new_line_no: Some(1),
                    },
                    DiffLine {
                        line_type: LineType::Deletion,
                        content: "old line".to_string(),
                        old_line_no: Some(2),
                        new_line_no: None,
                    },
                    DiffLine {
                        line_type: LineType::Addition,
                        content: "new line".to_string(),
                        old_line_no: None,
                        new_line_no: Some(2),
                    },
                ],
            }],
        }
    }

    // ── short_path tests ─────────────────────────────────────────────

    #[test]
    fn short_path_short_unchanged() {
        assert_eq!(short_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn short_path_long_returns_dir_and_filename() {
        assert_eq!(
            short_path("some/very/deeply/nested/directory/structure/file.rs"),
            "structure/file.rs"
        );
    }

    #[test]
    fn short_path_long_no_slash_returns_whole() {
        let path = "a_really_long_filename_without_slashes.rs";
        assert_eq!(short_path(path), path);
    }

    #[test]
    fn short_path_exactly_18_chars_unchanged() {
        let path = "123456789012345678";
        assert_eq!(path.len(), 18);
        assert_eq!(short_path(path), path);
    }

    #[test]
    fn short_path_19_chars_with_slash_returns_dir_file() {
        let path = "abc/defgh/123456789";
        assert_eq!(path.len(), 19);
        assert_eq!(short_path(path), "defgh/123456789");
    }

    // ── file_list_area tests ─────────────────────────────────────────

    #[test]
    fn file_list_area_standard_frame() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(area.width, DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn file_list_area_x_is_frame_width_minus_sidebar() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(area.x, 80 - DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn file_list_area_height_is_frame_minus_status_minus_commits() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame, DEFAULT_SIDEBAR_WIDTH);
        // 24 - 3 (status bar) - COMMIT_LIST_HEIGHT
        assert_eq!(area.height, 24 - 3 - COMMIT_LIST_HEIGHT);
    }

    // ── diff_area tests ──────────────────────────────────────────────

    #[test]
    fn diff_area_standard_frame() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert!(area.width > 0);
        assert!(area.height > 0);
    }

    #[test]
    fn diff_area_width_is_frame_minus_sidebar_minus_borders() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(area.width, 80 - DEFAULT_SIDEBAR_WIDTH - 2);
    }

    #[test]
    fn diff_area_height_is_frame_minus_status_minus_borders() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(area.height, 24 - 3 - 2);
    }

    #[test]
    fn diff_area_position() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(area.x, 1);
        assert_eq!(area.y, 1);
    }

    // ── render tests (TestBackend) ───────────────────────────────────

    #[test]
    fn render_no_files_shows_no_file_selected() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(vec![]);

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "No file selected"));
        assert!(buffer_contains(&buffer, "Click a file to view its diff"));
    }

    #[test]
    fn render_with_files_no_diff_shows_file_entries() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![
            make_test_file("src/main.rs", ChangeType::Modified),
            make_test_file("README.md", ChangeType::Added),
        ];
        let mut app = App::new(files);

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "src/main.rs"));
        assert!(buffer_contains(&buffer, "README.md"));
    }

    #[test]
    fn render_with_diff_shows_diff_content() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
        let mut app = App::new(files);
        app.select_file_with_diff(0, Some(make_test_diff()));

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "+new line"));
        assert!(buffer_contains(&buffer, "-old line"));
    }

    #[test]
    fn render_commenting_mode_shows_input() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
        let mut app = App::new(files);
        app.select_file_with_diff(0, Some(make_test_diff()));
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);
        app.start_input("my comment");

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "my comment"));
        assert!(buffer_contains(&buffer, "Enter: save")); // hint in card border
    }

    #[test]
    fn render_deleted_file_shows_d_indicator() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("removed.rs", ChangeType::Deleted)];
        let mut app = App::new(files);

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "D "));
    }

    #[test]
    fn render_renamed_file_shows_r_indicator() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("renamed.rs", ChangeType::Renamed)];
        let mut app = App::new(files);

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "R "));
    }

    #[test]
    fn render_file_with_comment_badge() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
        let mut app = App::new(files);
        app.select_file_with_diff(0, Some(make_test_diff()));
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![
                crate::app::ReviewComment {
                    line_index: 0,
                    text: "fix".to_string(),
                },
                crate::app::ReviewComment {
                    line_index: 1,
                    text: "also fix".to_string(),
                },
            ],
        );

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "[2]"));
    }

    #[test]
    fn render_hunk_header_line() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
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
                        content: "context".to_string(),
                        old_line_no: Some(1),
                        new_line_no: Some(1),
                    },
                ],
            }],
        };
        app.select_file_with_diff(0, Some(diff));

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "@@ -1,3 +1,4 @@"));
    }

    #[test]
    fn render_inline_comments_on_diff_lines() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
        let mut app = App::new(files);
        app.select_file_with_diff(0, Some(make_test_diff()));
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![crate::app::ReviewComment {
                line_index: 0,
                text: "review note here".to_string(),
            }],
        );

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "review note here"));
        assert!(buffer_contains(&buffer, "comment"));
    }

    #[test]
    fn render_summary_mode_status_bar() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(vec![]);
        app.start_input("my summary");
        app.mode = Mode::Summary;

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "summary:"));
        assert!(buffer_contains(&buffer, "my summary"));
    }

    #[test]
    fn render_commenting_mode_status_bar() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let files = vec![make_test_file("src/main.rs", ChangeType::Modified)];
        let mut app = App::new(files);
        app.select_file_with_diff(0, Some(make_test_diff()));
        app.mode = Mode::Commenting;
        app.commenting_line = Some(0);

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "typing comment"));
    }

    #[test]
    fn render_normal_mode_with_summary_done() {
        let backend = TestBackend::new(160, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(vec![]);
        app.summary = "my review".to_string();

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "summary: done"));
    }

    #[test]
    fn render_with_status_message_shows_in_status_bar() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(vec![]);
        app.status_message = Some("Review submitted!".to_string());

        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();

        assert!(buffer_contains(&buffer, "Review submitted!"));
    }
}
