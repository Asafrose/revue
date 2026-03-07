use crate::app::{App, Mode};
use crate::diff::LineType;
use crate::git::ChangeType;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, app: &mut App) {
    let [main_area, status_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(frame.area());

    let [diff_area, file_list_area] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(30),
    ])
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
                    Style::default().fg(indicator.1).add_modifier(Modifier::BOLD),
                ),
                Span::raw(short_path(&f.path)),
                Span::styled(
                    format!(" +{}-{}", f.additions, f.deletions),
                    Style::default().fg(Color::DarkGray),
                ),
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
    let file_comments = app
        .current_file
        .as_ref()
        .and_then(|f| app.comments.get(f));

    let all_diff_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();

    for (idx, diff_line) in all_diff_lines.iter().enumerate() {
        let (style, prefix) = match diff_line.line_type {
            LineType::Addition => (Style::default().fg(Color::Green), "+"),
            LineType::Deletion => (Style::default().fg(Color::Red), "-"),
            LineType::Context => (Style::default().fg(Color::White), " "),
            LineType::HunkHeader => (
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                "",
            ),
        };

        let line_no = match diff_line.line_type {
            LineType::HunkHeader => "    ".to_string(),
            _ => {
                let old = diff_line
                    .old_line_no
                    .map_or("  ".to_string(), |n| format!("{:>3}", n));
                let new = diff_line
                    .new_line_no
                    .map_or("  ".to_string(), |n| format!("{:>3}", n));
                format!("{} {}", old, new)
            }
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", line_no), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}{}", prefix, &diff_line.content), style),
        ]));

        // Render inline comments for this line
        if let Some(comments) = file_comments {
            for comment in comments.iter().filter(|c| c.line_index == idx) {
                lines.push(Line::from(vec![
                    Span::styled(
                        "       > ",
                        Style::default().fg(Color::Magenta),
                    ),
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
            lines.push(Line::from(vec![
                Span::styled(
                    "       > ",
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    app.input_buffer.clone(),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("_", Style::default().fg(Color::Yellow)),
            ]));
        }
    }

    // Apply scroll
    let visible_height = inner.height as usize;
    let scroll = app.diff_scroll.min(lines.len().saturating_sub(visible_height));
    let visible_lines: Vec<Line> = lines.into_iter().skip(scroll).take(visible_height).collect();

    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner);
}

fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let content = match app.mode {
        Mode::Normal => {
            let total_comments: usize = app.comments.values().map(|c| c.len()).sum();
            let summary_indicator = if app.summary.is_empty() { "" } else { " | summary: done" };
            format!(
                " click: select file/line | s: summary | S: submit | q: quit | comments: {}{}",
                total_comments, summary_indicator
            )
        }
        Mode::Commenting => " typing comment... | Enter: save | Esc: cancel".to_string(),
        Mode::Summary => format!(
            " summary: {}_  | Enter: save | Esc: cancel",
            app.input_buffer
        ),
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn short_path(path: &str) -> &str {
    if path.len() > 20 {
        path.rsplit('/').next().unwrap_or(path)
    } else {
        path
    }
}

/// Returns the area occupied by the file list panel.
pub fn file_list_area(frame_area: Rect) -> Rect {
    let [main_area, _status] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(frame_area);

    let [_diff, file_list] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(30),
    ])
    .areas(main_area);

    file_list
}

/// Returns the inner area of the diff panel (inside borders).
pub fn diff_area(frame_area: Rect) -> Rect {
    let [main_area, _status] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .areas(frame_area);

    let [diff, _file_list] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(30),
    ])
    .areas(main_area);

    Rect {
        x: diff.x + 1,
        y: diff.y + 1,
        width: diff.width.saturating_sub(2),
        height: diff.height.saturating_sub(2),
    }
}
