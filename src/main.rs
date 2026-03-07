mod app;
mod diff;
mod git;
mod review;
mod ui;

use anyhow::Result;
use app::{App, Mode};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;
use std::io::stderr;
use std::time::Duration;

fn main() -> Result<()> {
    let files = git::get_changed_files()?;

    execute!(stderr(), EnableMouseCapture)?;
    let mut terminal = ratatui::init();
    let mut app = App::new(files);

    if !app.files.is_empty() {
        app.select_file(0);
    }

    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    execute!(stderr(), DisableMouseCapture)?;
    result
}

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

fn handle_event(app: &mut App, event: Event, frame_size: ratatui::layout::Rect) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            app.status_message = None;
            match app.mode {
            Mode::Normal => match key.code {
                KeyCode::Char('q') => app.should_quit = true,
                KeyCode::Char('s') => {
                    app.mode = Mode::Summary;
                    app.input_buffer = app.summary.clone();
                }
                KeyCode::Char('S') => submit_review(app),
                KeyCode::Up | KeyCode::Char('k') => {
                    app.diff_scroll = app.diff_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.diff_scroll += 1;
                }
                KeyCode::Tab => {
                    let next = app.file_list_state.selected().map_or(0, |i| {
                        if i + 1 < app.files.len() { i + 1 } else { 0 }
                    });
                    app.select_file(next);
                }
                KeyCode::BackTab => {
                    let prev = app.file_list_state.selected().map_or(0, |i| {
                        if i == 0 { app.files.len().saturating_sub(1) } else { i - 1 }
                    });
                    app.select_file(prev);
                }
                KeyCode::Char('r') => {
                    if let Ok(files) = git::get_changed_files() {
                        app.files = files;
                        if !app.files.is_empty() {
                            app.select_file(0);
                        } else {
                            app.current_diff = None;
                            app.current_file = None;
                        }
                        app.status_message = Some("Refreshed file list".to_string());
                    }
                }
                _ => {}
            },
            Mode::Commenting => match key.code {
                KeyCode::Enter => app.submit_comment(),
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.commenting_line = None;
                    app.mode = Mode::Normal;
                }
                KeyCode::Char(c) => app.input_buffer.push(c),
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                _ => {}
            },
            Mode::Summary => match key.code {
                KeyCode::Enter => app.submit_summary(),
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.mode = Mode::Normal;
                }
                KeyCode::Char(c) => app.input_buffer.push(c),
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                _ => {}
            },
        }
        }
        Event::Mouse(mouse) => {
            if app.mode != Mode::Normal {
                return;
            }
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let col = mouse.column;
                    let row = mouse.row;

                    // Check if click is in file list
                    let file_area = ui::file_list_area(frame_size);
                    if col >= file_area.x
                        && col < file_area.x + file_area.width
                        && row >= file_area.y + 1
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
                            app.input_buffer.clear();
                        }
                    }
                }
                MouseEventKind::ScrollUp => {
                    app.diff_scroll = app.diff_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    app.diff_scroll += 3;
                }
                _ => {}
            }
        }
        _ => {}
    }
}

fn map_row_to_diff_line(app: &App, target_row: usize) -> Option<usize> {
    let diff = app.current_diff.as_ref()?;
    let file_comments = app
        .current_file
        .as_ref()
        .and_then(|f| app.comments.get(f));

    let all_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();
    let mut visual_row = 0;

    for (idx, _line) in all_lines.iter().enumerate() {
        if visual_row == target_row {
            return Some(idx);
        }
        visual_row += 1;
        if let Some(comments) = file_comments {
            visual_row += comments.iter().filter(|c| c.line_index == idx).count();
        }
    }
    None
}

fn submit_review(app: &mut App) {
    let output = review::format_review(app);
    if output.is_empty() {
        return;
    }
    if review::copy_to_clipboard(&output).is_ok() {
        app.status_message = Some("Review copied to clipboard!".to_string());
    }
}
