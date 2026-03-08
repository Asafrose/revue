use crate::app::{App, Mode};
use crate::diff::DiffLine;
use crate::diff::LineType;
use crate::git::ChangeType;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

/// Width of the file list sidebar
const FILE_LIST_WIDTH: u16 = 22;

pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, status_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame.area());

    let [diff_area, file_list_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(FILE_LIST_WIDTH)])
            .areas(main_area);

    render_file_list(frame, app, file_list_area);
    render_diff(frame, app, diff_area);
    render_status_bar(frame, app, status_area);
}

fn render_file_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .files
        .iter()
        .map(|f| {
            let indicator = match f.change_type {
                ChangeType::Added => ("A", Color::Green),
                ChangeType::Modified => ("M", Color::Yellow),
                ChangeType::Deleted => ("D", Color::Red),
                ChangeType::Renamed => ("R", Color::Cyan),
            };

            let comment_count = app.file_comment_count(&f.path);
            let comment_badge = if comment_count > 0 {
                format!(" [{}]", comment_count)
            } else {
                String::new()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", indicator.0),
                    Style::default()
                        .fg(indicator.1)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(short_path(&f.path)),
                Span::styled(comment_badge, Style::default().fg(Color::Magenta)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Files ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut app.file_list_state);
}

fn render_diff(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " {} ",
            app.current_file.as_deref().unwrap_or("No file selected")
        ))
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(diff) = &app.current_diff else {
        let hint = Paragraph::new("Click a file to view its diff");
        frame.render_widget(hint, inner);
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    let file_comments = app.current_file.as_ref().and_then(|f| app.comments.get(f));

    let all_diff_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();

    for (idx, diff_line) in all_diff_lines.iter().enumerate() {
        let (style, prefix) = match diff_line.line_type {
            LineType::Addition => (Style::default().fg(Color::Green), "+"),
            LineType::Deletion => (Style::default().fg(Color::Red), "-"),
            LineType::Context => (Style::default().fg(Color::White), " "),
            LineType::HunkHeader => (
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                "",
            ),
        };

        let line_no = match diff_line.line_type {
            LineType::HunkHeader => "   ".to_string(),
            _ => {
                let n = diff_line.new_line_no.or(diff_line.old_line_no);
                n.map_or("   ".to_string(), |n| format!("{:>3}", n))
            }
        };

        // Replace tabs with spaces and apply horizontal scroll
        let content = diff_line.content.replace('\t', "    ");
        let visible_content: String = content.chars().skip(app.diff_hscroll).collect();

        lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", line_no),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!("{}{}", prefix, visible_content), style),
        ]));

        // Render inline comments for this line
        if let Some(comments) = file_comments {
            for comment in comments.iter().filter(|c| c.line_index == idx) {
                lines.push(Line::from(vec![
                    Span::styled("       > ", Style::default().fg(Color::Magenta)),
                    Span::styled(
                        comment.text.clone(),
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
        }

        // Show input line if commenting on this line
        if app.mode == Mode::Commenting && app.commenting_line == Some(idx) {
            let text = app.input_text();
            lines.push(Line::from(vec![
                Span::styled("       > ", Style::default().fg(Color::Yellow)),
                Span::styled(text, Style::default().fg(Color::Yellow)),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ]));
        }
    }

    // Apply scroll (clamp and write back so future scroll-up works immediately)
    let total_lines = lines.len();
    let visible_height = inner.height as usize;
    let max_scroll = total_lines.saturating_sub(visible_height);
    app.diff_scroll = app.diff_scroll.min(max_scroll);
    let visible_lines: Vec<Line> = lines
        .into_iter()
        .skip(app.diff_scroll)
        .take(visible_height)
        .collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);

    // Render change map (scrollbar with colored change indicators)
    if total_lines > 0 {
        render_change_map(
            frame,
            &all_diff_lines,
            total_lines,
            visible_height,
            app.diff_scroll,
            inner,
        );
    }
}

/// Renders a 1-column change map on the right edge of the diff area.
/// Each row maps proportionally to the full file; additions are green,
/// deletions are red, and the current viewport is shown as a bright thumb.
fn render_change_map(
    frame: &mut Frame,
    diff_lines: &[&DiffLine],
    total_rendered: usize,
    visible_height: usize,
    scroll: usize,
    inner: Rect,
) {
    let height = inner.height as usize;
    if height == 0 || diff_lines.is_empty() {
        return;
    }

    let col = inner.x + inner.width.saturating_sub(1);
    let buf = frame.buffer_mut();

    // Thumb range (viewport indicator)
    let thumb_start = if total_rendered <= visible_height {
        0
    } else {
        scroll * height / total_rendered
    };
    let thumb_len = if total_rendered <= visible_height {
        height
    } else {
        (visible_height * height / total_rendered).max(1)
    };
    let thumb_end = (thumb_start + thumb_len).min(height);

    for row in 0..height {
        // Map this gutter row to a range of diff lines and find the most
        // significant change type (Addition > Deletion > Context).
        let range_start = row * diff_lines.len() / height;
        let range_end = ((row + 1) * diff_lines.len() / height).min(diff_lines.len());
        let mut has_addition = false;
        let mut has_deletion = false;
        for i in range_start..range_end.max(range_start + 1) {
            match diff_lines[i.min(diff_lines.len() - 1)].line_type {
                LineType::Addition => has_addition = true,
                LineType::Deletion => has_deletion = true,
                _ => {}
            }
        }

        let is_thumb = row >= thumb_start && row < thumb_end;

        let (ch, fg, bg) = if has_addition {
            if is_thumb {
                ("▐", Color::Green, Color::DarkGray)
            } else {
                ("▐", Color::Green, Color::Reset)
            }
        } else if has_deletion {
            if is_thumb {
                ("▐", Color::Red, Color::DarkGray)
            } else {
                ("▐", Color::Red, Color::Reset)
            }
        } else if is_thumb {
            ("█", Color::DarkGray, Color::Reset)
        } else {
            ("│", Color::Rgb(40, 40, 40), Color::Reset)
        };

        let y = inner.y + row as u16;
        if y < inner.y + inner.height {
            let cell = &mut buf[(col, y)];
            cell.set_symbol(ch);
            cell.set_style(Style::default().fg(fg).bg(bg));
        }
    }
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let content = match app.mode {
        Mode::Normal => {
            if let Some(ref msg) = app.status_message {
                msg.clone()
            } else {
                let total_comments: usize = app.comments.values().map(|c| c.len()).sum();
                let summary_indicator = if app.summary.is_empty() {
                    ""
                } else {
                    " | summary: done"
                };
                format!(
                    " Tab: files | j/k: scroll | h/l: pan | s: summary | S: submit | q: quit | {}{}",
                    total_comments, summary_indicator
                )
            }
        }
        Mode::Commenting => " typing comment... | Enter: save | Esc: cancel".to_string(),
        Mode::Summary => format!(
            " summary: {}_  | Enter: save | Esc: cancel",
            app.input_text()
        ),
    };

    let style = if app.status_message.is_some() && app.mode == Mode::Normal {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    let paragraph = Paragraph::new(content)
        .style(style)
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
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

/// Returns the area occupied by the file list panel.
pub fn file_list_area(frame_area: Rect) -> Rect {
    let [main_area, _status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame_area);

    let [_diff, file_list] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(FILE_LIST_WIDTH)])
            .areas(main_area);

    file_list
}

/// Returns the inner area of the diff panel (inside borders).
pub fn diff_area(frame_area: Rect) -> Rect {
    let [main_area, _status] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame_area);

    let [diff, _file_list] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(FILE_LIST_WIDTH)])
            .areas(main_area);

    Rect {
        x: diff.x + 1,
        y: diff.y + 1,
        width: diff.width.saturating_sub(2),
        height: diff.height.saturating_sub(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
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
        // No slash means rsplit('/').next() returns the whole string
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
        // 19 characters with slashes — shows dir/file
        let path = "abc/defgh/123456789";
        assert_eq!(path.len(), 19);
        assert_eq!(short_path(path), "defgh/123456789");
    }

    // ── file_list_area tests ─────────────────────────────────────────

    #[test]
    fn file_list_area_standard_frame() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame);
        assert_eq!(area.width, FILE_LIST_WIDTH);
    }

    #[test]
    fn file_list_area_x_is_frame_width_minus_sidebar() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame);
        assert_eq!(area.x, 80 - FILE_LIST_WIDTH);
    }

    #[test]
    fn file_list_area_height_is_frame_height_minus_3() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = file_list_area(frame);
        assert_eq!(area.height, 24 - 3);
    }

    // ── diff_area tests ──────────────────────────────────────────────

    #[test]
    fn diff_area_standard_frame() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame);
        // inner rect: inside the diff block borders
        assert!(area.width > 0);
        assert!(area.height > 0);
    }

    #[test]
    fn diff_area_width_is_frame_minus_sidebar_minus_borders() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame);
        assert_eq!(area.width, 80 - FILE_LIST_WIDTH - 2);
    }

    #[test]
    fn diff_area_height_is_frame_minus_status_minus_borders() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame);
        // main height = 24 - 3 = 21, inner = 21 - 2 = 19
        assert_eq!(area.height, 24 - 3 - 2);
    }

    #[test]
    fn diff_area_position() {
        let frame = Rect::new(0, 0, 80, 24);
        let area = diff_area(frame);
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
        assert!(buffer_contains(&buffer, "_")); // cursor
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
        assert!(buffer_contains(&buffer, ">"));
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
