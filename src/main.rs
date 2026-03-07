use anyhow::Result;
use ratatui::{DefaultTerminal, Frame};
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEventKind, EnableMouseCapture, DisableMouseCapture,
};
use ratatui::crossterm::execute;
use std::io::{stdout, stderr};
use std::time::Duration;

fn main() -> Result<()> {
    // Enable mouse capture
    execute!(stderr(), EnableMouseCapture)?;
    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    execute!(stderr(), DisableMouseCapture)?;
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    loop {
        terminal.draw(render)?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
    }
}

fn render(frame: &mut Frame) {
    use ratatui::widgets::{Block, Paragraph};
    let p = Paragraph::new("revue — press q to quit")
        .block(Block::bordered().title("revue"));
    frame.render_widget(p, frame.area());
}
