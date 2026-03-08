use super::comment_card::CommentCard;
use crate::app::{App, Mode};
use crate::diff::DiffLine;
use crate::diff::LineType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, StatefulWidget, Widget};
use syntect::easy::HighlightLines;
use syntect::highlighting::Style as SynStyle;

pub struct DiffWidget;

/// Convert a syntect color to a ratatui color.
fn syn_to_rat_fg(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Build a line number span.
fn line_no_span(diff_line: &DiffLine) -> Span<'static> {
    let text = match diff_line.line_type {
        LineType::HunkHeader => "   ".to_string(),
        _ => {
            let n = diff_line.new_line_no.or(diff_line.old_line_no);
            n.map_or("   ".to_string(), |n| format!("{:>3}", n))
        }
    };
    Span::styled(format!("{} ", text), Style::default().fg(Color::DarkGray))
}

/// Build spans for a syntax-highlighted content line with a diff prefix.
fn highlighted_spans(
    prefix: &str,
    content: &str,
    hscroll: usize,
    regions: &[(SynStyle, &str)],
    bg: Option<Color>,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    // Prefix span (+/-/space)
    let prefix_style = match prefix {
        "+" => Style::default().fg(Color::Green),
        "-" => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::DarkGray),
    };
    let prefix_style = if let Some(bg) = bg {
        prefix_style.bg(bg)
    } else {
        prefix_style
    };
    spans.push(Span::styled(prefix.to_string(), prefix_style));

    // Map syntax regions onto the visible (hscroll-adjusted) content
    let visible: String = content.chars().skip(hscroll).collect();
    if visible.is_empty() {
        return spans;
    }

    // Walk through regions, skipping hscroll chars
    let mut chars_skipped = 0;
    for (style, text) in regions {
        let region_len = text.chars().count();
        if chars_skipped + region_len <= hscroll {
            chars_skipped += region_len;
            continue;
        }

        let skip_in_region = hscroll.saturating_sub(chars_skipped);
        let visible_text: String = text.chars().skip(skip_in_region).collect();
        chars_skipped += skip_in_region;

        if !visible_text.is_empty() {
            let mut span_style = Style::default().fg(syn_to_rat_fg(style.foreground));
            if let Some(bg) = bg {
                span_style = span_style.bg(bg);
            }
            spans.push(Span::styled(visible_text, span_style));
        }
        chars_skipped += region_len - skip_in_region;
    }

    spans
}

/// Build spans for a line without syntax highlighting (fallback).
fn plain_spans(prefix: &str, content: &str, hscroll: usize, style: Style) -> Vec<Span<'static>> {
    let visible_content: String = content.chars().skip(hscroll).collect();
    vec![Span::styled(
        format!("{}{}", prefix, visible_content),
        style,
    )]
}

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

        // Set up syntax highlighter based on file extension
        let highlighter = state.current_file.as_ref().and_then(|path| {
            let ext = path.rsplit('.').next()?;
            let syntax = state.syntax_set.find_syntax_by_extension(ext)?;
            let theme = &state.theme_set.themes["base16-ocean.dark"];
            Some(HighlightLines::new(syntax, theme))
        });
        let mut highlighter = highlighter;

        // Card width for comment boxes (inner width minus padding)
        let card_width = inner.width.saturating_sub(8) as usize;

        let mut lines: Vec<Line> = Vec::new();
        let file_comments = state
            .current_file
            .as_ref()
            .and_then(|f| state.comments.get(f));

        let all_diff_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();

        for (idx, diff_line) in all_diff_lines.iter().enumerate() {
            let content = diff_line.content.replace('\t', "    ");

            let mut spans = vec![line_no_span(diff_line)];

            match diff_line.line_type {
                LineType::HunkHeader => {
                    spans.push(Span::styled(
                        content.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                    // Reset highlighter state at hunk boundaries
                    if let Some(ref mut h) = highlighter {
                        // Re-create to reset parse state
                        if let Some(path) = &state.current_file {
                            if let Some(ext) = path.rsplit('.').next() {
                                if let Some(syntax) = state.syntax_set.find_syntax_by_extension(ext)
                                {
                                    let theme = &state.theme_set.themes["base16-ocean.dark"];
                                    *h = HighlightLines::new(syntax, theme);
                                }
                            }
                        }
                    }
                }
                LineType::Addition | LineType::Deletion | LineType::Context => {
                    let prefix = match diff_line.line_type {
                        LineType::Addition => "+",
                        LineType::Deletion => "-",
                        _ => " ",
                    };

                    let bg = match diff_line.line_type {
                        LineType::Addition => Some(Color::Rgb(0, 40, 0)),
                        LineType::Deletion => Some(Color::Rgb(40, 0, 0)),
                        _ => None,
                    };

                    // Feed the line to the highlighter (needs trailing newline)
                    let line_for_highlight = format!("{}\n", content);
                    if let Some(ref mut h) = highlighter {
                        if let Ok(regions) =
                            h.highlight_line(&line_for_highlight, &state.syntax_set)
                        {
                            spans.extend(highlighted_spans(
                                prefix,
                                &content,
                                state.diff_hscroll,
                                &regions,
                                bg,
                            ));
                        } else {
                            let style = match diff_line.line_type {
                                LineType::Addition => Style::default().fg(Color::Green),
                                LineType::Deletion => Style::default().fg(Color::Red),
                                _ => Style::default().fg(Color::White),
                            };
                            spans.extend(plain_spans(prefix, &content, state.diff_hscroll, style));
                        }
                    } else {
                        let style = match diff_line.line_type {
                            LineType::Addition => Style::default().fg(Color::Green),
                            LineType::Deletion => Style::default().fg(Color::Red),
                            _ => Style::default().fg(Color::White),
                        };
                        spans.extend(plain_spans(prefix, &content, state.diff_hscroll, style));
                    }
                }
            }

            lines.push(Line::from(spans));

            // Render inline comments as cards
            if let Some(comments) = file_comments {
                for comment in comments.iter().filter(|c| c.line_index == idx) {
                    let card = CommentCard::new(&comment.text, Color::Magenta, card_width);
                    lines.extend(card.to_lines());
                }
            }

            // Show input card if commenting on this line
            if state.mode == Mode::Commenting && state.commenting_line == Some(idx) {
                let text = state.input_text();
                let cursor_visible =
                    state.cursor_blink_start.elapsed().as_millis() % 1000 < 500;
                let (crow, ccol) = state
                    .textarea
                    .as_ref()
                    .map(|ta| ta.cursor())
                    .unwrap_or((0, 0));
                let card = CommentCard::new(&text, Color::Yellow, card_width)
                    .hint("⏎ save │ Alt+⏎ newline │ Esc cancel")
                    .cursor(crow, ccol, cursor_visible);
                lines.extend(card.to_lines());
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
