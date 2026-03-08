use crate::app::{App, Mode};
use crate::diff::DiffLine;
use crate::diff::LineType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Widget};

pub struct DiffWidget;

impl StatefulWidget for DiffWidget {
    type State = App;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut App) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(
                " {} ",
                state.current_file.as_deref().unwrap_or("No file selected")
            ))
            .border_style(Style::default().fg(Color::DarkGray));

        let inner = block.inner(area);
        block.render(area, buf);

        let Some(diff) = &state.current_diff else {
            Paragraph::new("Click a file to view its diff").render(inner, buf);
            return;
        };

        let mut lines: Vec<Line> = Vec::new();
        let file_comments = state
            .current_file
            .as_ref()
            .and_then(|f| state.comments.get(f));

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
            let visible_content: String = content.chars().skip(state.diff_hscroll).collect();

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
            if state.mode == Mode::Commenting && state.commenting_line == Some(idx) {
                let text = state.input_text();
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
        state.diff_scroll = state.diff_scroll.min(max_scroll);
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(state.diff_scroll)
            .take(visible_height)
            .collect();

        Paragraph::new(visible_lines).render(inner, buf);

        // Render change map (scrollbar with colored change indicators)
        if total_lines > 0 {
            render_change_map(
                buf,
                &all_diff_lines,
                total_lines,
                visible_height,
                state.diff_scroll,
                inner,
            );
        }
    }
}

/// Renders a 1-column change map on the right edge of the diff area.
/// Each row maps proportionally to the full file; additions are green,
/// deletions are red, and the current viewport is shown as a bright thumb.
fn render_change_map(
    buf: &mut Buffer,
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
