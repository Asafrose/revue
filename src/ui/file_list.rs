use crate::app::App;
use crate::git::ChangeType;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, StatefulWidget};

use super::short_path;

pub struct FileListWidget;

impl StatefulWidget for FileListWidget {
    type State = App;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut App) {
        let items: Vec<ListItem> = state
            .files
            .iter()
            .map(|f| {
                let indicator = match f.change_type {
                    ChangeType::Added => ("A", Color::Green),
                    ChangeType::Modified => ("M", Color::Yellow),
                    ChangeType::Deleted => ("D", Color::Red),
                    ChangeType::Renamed => ("R", Color::Cyan),
                };

                let comment_count = state.file_comment_count(&f.path);
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

        StatefulWidget::render(list, area, buf, &mut state.file_list_state);
    }
}
