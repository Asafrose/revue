use crate::app::{App, Mode};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

pub struct StatusBarWidget<'a> {
    app: &'a App,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(app: &'a App) -> Self {
        Self { app }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));

        let content = match self.app.mode {
            Mode::Normal => {
                if let Some(ref msg) = self.app.status_message {
                    msg.clone()
                } else {
                    let total_comments: usize = self.app.comments.values().map(|c| c.len()).sum();
                    let summary_indicator = if self.app.summary.is_empty() {
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
                self.app.input_text()
            ),
        };

        let style = if self.app.status_message.is_some() && self.app.mode == Mode::Normal {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };

        Paragraph::new(content)
            .style(style)
            .block(block)
            .wrap(Wrap { trim: true })
            .render(area, buf);
    }
}
