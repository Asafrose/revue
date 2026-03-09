use crate::app::App;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, StatefulWidget};

pub struct CommitListWidget;

impl StatefulWidget for CommitListWidget {
    type State = App;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut App) {
        let items: Vec<ListItem> = state
            .commits
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let selected = state.selected_commits.get(i).copied().unwrap_or(false);
                let check = if selected { "●" } else { "○" };
                let check_color = if selected {
                    Color::Green
                } else {
                    Color::DarkGray
                };

                let msg: String = c.message.chars().take(16).collect();

                let line = Line::from(vec![
                    Span::styled(format!("{} ", check), Style::default().fg(check_color)),
                    Span::styled(
                        format!("{} ", c.short_id),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::raw(msg),
                ]);

                ListItem::new(line)
            })
            .collect();

        let title = format!(" Commits ({}) ", state.commits.len());
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );

        StatefulWidget::render(list, area, buf, &mut state.commit_list_state);
    }
}
