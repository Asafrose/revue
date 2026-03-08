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
}

impl<'a> CommentCard<'a> {
    pub fn new(text: &'a str, color: Color, width: usize) -> Self {
        Self {
            text,
            color,
            hint: None,
            indent: "     ",
            width,
        }
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = Some(hint);
        self
    }

    /// Produce styled `Line` entries for embedding in a Paragraph-based flow.
    pub fn to_lines(&self) -> Vec<Line<'static>> {
        let style = Style::default().fg(self.color);
        let inner_w = self.width.saturating_sub(2);
        let mut lines = Vec::new();

        // Top border
        let title = if let Some(h) = self.hint {
            format!(" {} ", h)
        } else {
            " comment ".to_string()
        };
        let border_fill = inner_w.saturating_sub(title.len());
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
        for content in &content_lines {
            let truncated: String = content.chars().take(inner_w.saturating_sub(2)).collect();
            let padding = inner_w
                .saturating_sub(2)
                .saturating_sub(truncated.chars().count());
            lines.push(Line::from(vec![
                Span::styled(format!("{}│ ", self.indent), style),
                Span::styled(
                    format!("{}{}", truncated, " ".repeat(padding)),
                    style.add_modifier(Modifier::ITALIC),
                ),
                Span::styled(" │", style),
            ]));
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
