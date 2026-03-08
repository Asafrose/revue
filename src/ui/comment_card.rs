use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

pub struct CommentCard<'a> {
    text: &'a str,
    color: Color,
    hint: Option<&'a str>,
    indent: &'a str,
    width: usize,
    /// Optional cursor position (row, col) within the text, with visibility flag.
    cursor: Option<(usize, usize, bool)>,
}

impl<'a> CommentCard<'a> {
    pub fn new(text: &'a str, color: Color, width: usize) -> Self {
        Self {
            text,
            color,
            hint: None,
            indent: "     ",
            width,
            cursor: None,
        }
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    pub fn cursor(mut self, row: usize, col: usize, visible: bool) -> Self {
        self.cursor = Some((row, col, visible));
        self
    }

    /// Produce styled `Line` entries for embedding in a Paragraph-based flow.
    pub fn to_lines(&self) -> Vec<Line<'static>> {
        let style = Style::default().fg(self.color);
        let inner_w = self.width.saturating_sub(2);
        let mut lines = Vec::new();

        // Top border
        let title_raw = if let Some(h) = self.hint {
            format!(" {} ", h)
        } else {
            " comment ".to_string()
        };
        let max_title: usize = inner_w.saturating_sub(1);
        let title: String = title_raw.chars().take(max_title).collect();
        let title_len = title.chars().count();
        let border_fill = inner_w.saturating_sub(title_len);
        lines.push(Line::from(Span::styled(
            format!("{}┌{}{}┐", self.indent, title, "─".repeat(border_fill)),
            style,
        )));

        // Content lines
        let content_lines: Vec<&str> = if self.text.is_empty() {
            vec![""]
        } else {
            self.text.lines().collect()
        };
        let content_w = inner_w.saturating_sub(2);
        for (line_idx, content) in content_lines.iter().enumerate() {
            let truncated: String = content.chars().take(content_w).collect();
            let trunc_len = truncated.chars().count();
            let padding = content_w.saturating_sub(trunc_len);

            let has_cursor = matches!(self.cursor, Some((r, _, true)) if r == line_idx);

            if has_cursor {
                let (_, col, _) = self.cursor.unwrap();
                let chars: Vec<char> = truncated.chars().collect();
                let col = col.min(chars.len());

                let before: String = chars[..col].iter().collect();
                let cursor_ch = if col < chars.len() {
                    chars[col].to_string()
                } else {
                    " ".to_string()
                };
                let after: String = if col < chars.len() {
                    chars[col + 1..].iter().collect()
                } else {
                    String::new()
                };
                let after_padding = if col < chars.len() {
                    padding
                } else {
                    padding.saturating_sub(1)
                };

                let content_style = style.add_modifier(Modifier::ITALIC);
                let cursor_style = Style::default().fg(Color::Black).bg(self.color);

                lines.push(Line::from(vec![
                    Span::styled(format!("{}│ ", self.indent), style),
                    Span::styled(before, content_style),
                    Span::styled(cursor_ch, cursor_style),
                    Span::styled(
                        format!("{}{}", after, " ".repeat(after_padding)),
                        content_style,
                    ),
                    Span::styled(" │", style),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(format!("{}│ ", self.indent), style),
                    Span::styled(
                        format!("{}{}", truncated, " ".repeat(padding)),
                        style.add_modifier(Modifier::ITALIC),
                    ),
                    Span::styled(" │", style),
                ]));
            }
        }

        // Bottom border
        lines.push(Line::from(Span::styled(
            format!("{}└{}┘", self.indent, "─".repeat(inner_w)),
            style,
        )));

        lines
    }
}

impl Widget for CommentCard<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 4 || area.height < 3 {
            return;
        }

        let rendered_lines = self.to_lines();
        for (i, line) in rendered_lines.iter().enumerate() {
            let row = area.y + i as u16;
            if row >= area.y + area.height {
                break;
            }
            buf.set_line(area.x, row, line, area.width);
        }
    }
}
