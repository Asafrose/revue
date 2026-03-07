# Revue v1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a mouse-heavy TUI code review sidecar that diffs against main, supports inline comments, and copies structured feedback to clipboard.

**Architecture:** Single-binary Rust app. Shells out to `git` for diffs. Parses unified diff into structured data. Renders with ratatui (crossterm backend) in a two-panel layout (diff view + right sidebar file list). Mouse-first interaction model.

**Tech Stack:** Rust, ratatui, crossterm (re-exported by ratatui), arboard (clipboard), syntect (syntax highlighting)

---

### Task 1: Project Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize cargo project**

Run: `cargo init --name revue`

**Step 2: Add dependencies to Cargo.toml**

Replace the `[dependencies]` section in `Cargo.toml`:

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
arboard = "3"
syntect = "5"
anyhow = "1"

[dev-dependencies]
pretty_assertions = "1"
```

**Step 3: Write minimal app that launches and quits**

Replace `src/main.rs` with:

```rust
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
```

**Step 4: Verify it compiles and runs**

Run: `cargo build`
Expected: Compiles with no errors.

Run: `cargo run` (then press q)
Expected: TUI appears with bordered box, quits on 'q'.

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/
git commit -m "feat: project scaffolding with ratatui skeleton"
```

---

### Task 2: Git Diff Integration

**Files:**
- Create: `src/git.rs`
- Modify: `src/main.rs` (add `mod git;`)
- Create: `tests/git_test.rs`

**Step 1: Write tests for git module**

Create `tests/git_test.rs`:

```rust
use std::process::Command;

// Integration test: run in an actual git repo
#[test]
fn test_git_diff_stat_parses() {
    // This test runs against the revue repo itself
    let output = Command::new("git")
        .args(["diff", "--stat", "main...HEAD"])
        .output()
        .expect("git command failed");
    // Should not error, even if no diff
    assert!(output.status.success());
}
```

**Step 2: Run test to verify it passes (sanity check)**

Run: `cargo test --test git_test`
Expected: PASS

**Step 3: Implement git module**

Create `src/git.rs`:

```rust
use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub change_type: ChangeType,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}

pub fn get_changed_files() -> Result<Vec<ChangedFile>> {
    let output = Command::new("git")
        .args(["diff", "--numstat", "--diff-filter=ADMR", "main"])
        .output()
        .context("Failed to run git diff")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {}", err);
    }

    let stdout = String::from_utf8(output.stdout)?;
    let mut files = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let additions = parts[0].parse().unwrap_or(0);
        let deletions = parts[1].parse().unwrap_or(0);
        let path = parts[2].to_string();

        files.push(ChangedFile {
            path,
            change_type: ChangeType::Modified, // refined below
            additions,
            deletions,
        });
    }

    // Get change types
    let type_output = Command::new("git")
        .args(["diff", "--name-status", "--diff-filter=ADMR", "main"])
        .output()
        .context("Failed to run git diff --name-status")?;

    let type_stdout = String::from_utf8(type_output.stdout)?;
    for line in type_stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let change_type = match parts[0].chars().next() {
            Some('A') => ChangeType::Added,
            Some('D') => ChangeType::Deleted,
            Some('R') => ChangeType::Renamed,
            _ => ChangeType::Modified,
        };
        let path = parts.last().unwrap().to_string();
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            f.change_type = change_type;
        }
    }

    Ok(files)
}

pub fn get_file_diff(path: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "main", "--", path])
        .output()
        .context("Failed to run git diff for file")?;

    Ok(String::from_utf8(output.stdout)?)
}
```

**Step 4: Add module to main.rs**

Add `mod git;` at the top of `src/main.rs`.

**Step 5: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 6: Commit**

```bash
git add src/git.rs tests/git_test.rs src/main.rs
git commit -m "feat: git diff integration module"
```

---

### Task 3: Unified Diff Parser

**Files:**
- Create: `src/diff.rs`
- Modify: `src/main.rs` (add `mod diff;`)

**Step 1: Write tests for diff parser**

Add tests at the bottom of `src/diff.rs` (we'll write the module with inline tests):

The parser needs to handle unified diff format:
```
--- a/file.rs
+++ b/file.rs
@@ -10,6 +10,7 @@ fn foo() {
 context line
-removed line
+added line
 context line
```

**Step 2: Write diff parser with tests**

Create `src/diff.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum LineType {
    Context,
    Addition,
    Deletion,
    HunkHeader,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: LineType,
    pub content: String,
    pub old_line_no: Option<usize>,
    pub new_line_no: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub hunks: Vec<Hunk>,
}

pub fn parse_diff(raw: &str) -> FileDiff {
    let mut hunks = Vec::new();
    let mut current_lines: Vec<DiffLine> = Vec::new();
    let mut current_header = String::new();
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;

    for line in raw.lines() {
        if line.starts_with("@@") {
            // Save previous hunk
            if !current_lines.is_empty() {
                hunks.push(Hunk {
                    header: current_header.clone(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            current_header = line.to_string();

            // Parse line numbers from @@ -old,count +new,count @@
            if let Some((o, n)) = parse_hunk_header(line) {
                old_line = o;
                new_line = n;
            }

            current_lines.push(DiffLine {
                line_type: LineType::HunkHeader,
                content: line.to_string(),
                old_line_no: None,
                new_line_no: None,
            });
        } else if line.starts_with("---") || line.starts_with("+++") || line.starts_with("diff ") || line.starts_with("index ") {
            // Skip file headers
            continue;
        } else if line.starts_with('+') {
            current_lines.push(DiffLine {
                line_type: LineType::Addition,
                content: line[1..].to_string(),
                old_line_no: None,
                new_line_no: Some(new_line),
            });
            new_line += 1;
        } else if line.starts_with('-') {
            current_lines.push(DiffLine {
                line_type: LineType::Deletion,
                content: line[1..].to_string(),
                old_line_no: Some(old_line),
                new_line_no: None,
            });
            old_line += 1;
        } else if line.starts_with(' ') || line.is_empty() {
            let content = if line.is_empty() { "" } else { &line[1..] };
            current_lines.push(DiffLine {
                line_type: LineType::Context,
                content: content.to_string(),
                old_line_no: Some(old_line),
                new_line_no: Some(new_line),
            });
            old_line += 1;
            new_line += 1;
        }
    }

    // Save last hunk
    if !current_lines.is_empty() {
        hunks.push(Hunk {
            header: current_header,
            lines: current_lines,
        });
    }

    FileDiff { hunks }
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    // @@ -10,6 +10,7 @@ optional context
    let line = line.strip_prefix("@@ ")?;
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let old = parts[0].strip_prefix('-')?;
    let new = parts[1].strip_prefix('+')?;

    let old_start: usize = old.split(',').next()?.parse().ok()?;
    let new_start: usize = new.split(',').next()?.parse().ok()?;

    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DIFF: &str = r#"diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,6 +10,7 @@ fn main() {
     let x = 1;
     let y = 2;
-    let z = x + y;
+    let z = x * y;
+    println!("{}", z);
     Ok(())
 }
"#;

    #[test]
    fn test_parse_diff_hunks() {
        let diff = parse_diff(SAMPLE_DIFF);
        assert_eq!(diff.hunks.len(), 1);
    }

    #[test]
    fn test_parse_diff_line_types() {
        let diff = parse_diff(SAMPLE_DIFF);
        let hunk = &diff.hunks[0];
        // hunk header + 2 context + 1 deletion + 2 additions + 1 context + 1 context (empty closing)
        assert!(hunk.lines.len() >= 6);
        assert_eq!(hunk.lines[0].line_type, LineType::HunkHeader);
        assert_eq!(hunk.lines[1].line_type, LineType::Context);
        assert_eq!(hunk.lines[2].line_type, LineType::Context);
        assert_eq!(hunk.lines[3].line_type, LineType::Deletion);
        assert_eq!(hunk.lines[4].line_type, LineType::Addition);
        assert_eq!(hunk.lines[5].line_type, LineType::Addition);
    }

    #[test]
    fn test_parse_diff_line_numbers() {
        let diff = parse_diff(SAMPLE_DIFF);
        let hunk = &diff.hunks[0];
        // First context line: old=10, new=10
        assert_eq!(hunk.lines[1].old_line_no, Some(10));
        assert_eq!(hunk.lines[1].new_line_no, Some(10));
        // Deletion: old=12, new=None
        assert_eq!(hunk.lines[3].old_line_no, Some(12));
        assert_eq!(hunk.lines[3].new_line_no, None);
        // First addition: old=None, new=12
        assert_eq!(hunk.lines[4].old_line_no, None);
        assert_eq!(hunk.lines[4].new_line_no, Some(12));
    }

    #[test]
    fn test_parse_hunk_header_numbers() {
        let result = parse_hunk_header("@@ -10,6 +10,7 @@ fn main() {");
        assert_eq!(result, Some((10, 10)));
    }

    #[test]
    fn test_empty_diff() {
        let diff = parse_diff("");
        assert_eq!(diff.hunks.len(), 0);
    }
}
```

**Step 3: Add module to main.rs**

Add `mod diff;` to `src/main.rs`.

**Step 4: Run tests**

Run: `cargo test`
Expected: All diff parser tests pass.

**Step 5: Commit**

```bash
git add src/diff.rs src/main.rs
git commit -m "feat: unified diff parser with tests"
```

---

### Task 4: App State & Data Model

**Files:**
- Create: `src/app.rs`
- Modify: `src/main.rs`

**Step 1: Create app state**

Create `src/app.rs`:

```rust
use crate::diff::FileDiff;
use crate::git::ChangedFile;
use ratatui::widgets::ListState;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ReviewComment {
    pub line_index: usize, // index into the diff lines
    pub text: String,
}

pub struct App {
    pub files: Vec<ChangedFile>,
    pub file_list_state: ListState,
    pub current_diff: Option<FileDiff>,
    pub current_file: Option<String>,
    pub comments: HashMap<String, Vec<ReviewComment>>, // file path -> comments
    pub summary: String,
    pub mode: Mode,
    pub input_buffer: String,
    pub diff_scroll: usize,
    pub commenting_line: Option<usize>, // which diff line index we're commenting on
    pub should_quit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Commenting,
    Summary,
}

impl App {
    pub fn new(files: Vec<ChangedFile>) -> Self {
        let mut file_list_state = ListState::default();
        if !files.is_empty() {
            file_list_state.select(Some(0));
        }
        Self {
            files,
            file_list_state,
            current_diff: None,
            current_file: None,
            comments: HashMap::new(),
            summary: String::new(),
            mode: Mode::Normal,
            input_buffer: String::new(),
            diff_scroll: 0,
            commenting_line: None,
            should_quit: false,
        }
    }

    pub fn selected_file(&self) -> Option<&ChangedFile> {
        self.file_list_state.selected().and_then(|i| self.files.get(i))
    }

    pub fn select_file(&mut self, index: usize) {
        if index < self.files.len() {
            self.file_list_state.select(Some(index));
            let path = self.files[index].path.clone();
            match crate::git::get_file_diff(&path) {
                Ok(raw) => {
                    self.current_diff = Some(crate::diff::parse_diff(&raw));
                    self.current_file = Some(path);
                    self.diff_scroll = 0;
                }
                Err(_) => {
                    self.current_diff = None;
                    self.current_file = None;
                }
            }
        }
    }

    pub fn submit_comment(&mut self) {
        if let (Some(line_idx), Some(file)) = (self.commenting_line, &self.current_file) {
            if !self.input_buffer.is_empty() {
                let comment = ReviewComment {
                    line_index: line_idx,
                    text: self.input_buffer.clone(),
                };
                self.comments
                    .entry(file.clone())
                    .or_default()
                    .push(comment);
            }
        }
        self.input_buffer.clear();
        self.commenting_line = None;
        self.mode = Mode::Normal;
    }

    pub fn submit_summary(&mut self) {
        self.summary = self.input_buffer.clone();
        self.input_buffer.clear();
        self.mode = Mode::Normal;
    }

    pub fn file_comment_count(&self, path: &str) -> usize {
        self.comments.get(path).map_or(0, |c| c.len())
    }
}
```

**Step 2: Add module to main.rs**

Add `mod app;` to `src/main.rs`.

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Commit**

```bash
git add src/app.rs src/main.rs
git commit -m "feat: app state and data model"
```

---

### Task 5: Layout & File List Panel

**Files:**
- Create: `src/ui.rs`
- Modify: `src/main.rs`

**Step 1: Create UI rendering module**

Create `src/ui.rs`:

```rust
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
                        "       💬 ",
                        Style::default().fg(Color::Magenta),
                    ),
                    Span::styled(
                        &comment.text,
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
                    &app.input_buffer,
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled("▌", Style::default().fg(Color::Yellow)),
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
            let summary_indicator = if app.summary.is_empty() { "" } else { " | summary: ✓" };
            format!(
                " click: select file/comment line | s: summary | S: submit review | q: quit | comments: {}{}",
                total_comments, summary_indicator
            )
        }
        Mode::Commenting => " typing comment... | Enter: save | Esc: cancel".to_string(),
        Mode::Summary => format!(
            " summary: {}▌ | Enter: save | Esc: cancel",
            app.input_buffer
        ),
    };

    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn short_path(path: &str) -> &str {
    // Show just the filename if path is long
    if path.len() > 20 {
        path.rsplit('/').next().unwrap_or(path)
    } else {
        path
    }
}

/// Returns the area occupied by the file list panel.
/// Used by event handling to detect clicks in the file list.
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

/// Returns the area occupied by the diff panel.
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

    // Account for the border (1 line top)
    let inner = Rect {
        x: diff.x + 1,
        y: diff.y + 1,
        width: diff.width.saturating_sub(2),
        height: diff.height.saturating_sub(2),
    };
    inner
}
```

**Step 2: Update main.rs to use UI and app**

Replace `src/main.rs`:

```rust
mod app;
mod diff;
mod git;
mod ui;

use anyhow::Result;
use app::{App, Mode};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;
use std::io::stderr;
use std::time::Duration;

fn main() -> Result<()> {
    let files = git::get_changed_files()?;

    execute!(stderr(), EnableMouseCapture)?;
    let mut terminal = ratatui::init();
    let mut app = App::new(files);

    // Auto-select first file
    if !app.files.is_empty() {
        app.select_file(0);
    }

    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    execute!(stderr(), DisableMouseCapture)?;
    result
}

fn run(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            handle_event(app, ev, terminal.size()?);
            if app.should_quit {
                return Ok(());
            }
        }
    }
}

fn handle_event(app: &mut App, event: Event, frame_size: ratatui::layout::Rect) {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => match app.mode {
            Mode::Normal => match key.code {
                KeyCode::Char('q') => app.should_quit = true,
                KeyCode::Char('s') => {
                    app.mode = Mode::Summary;
                    app.input_buffer = app.summary.clone();
                }
                KeyCode::Char('S') => submit_review(app),
                KeyCode::Up | KeyCode::Char('k') => {
                    app.diff_scroll = app.diff_scroll.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    app.diff_scroll += 1;
                }
                _ => {}
            },
            Mode::Commenting => match key.code {
                KeyCode::Enter => app.submit_comment(),
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.commenting_line = None;
                    app.mode = Mode::Normal;
                }
                KeyCode::Char(c) => app.input_buffer.push(c),
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                _ => {}
            },
            Mode::Summary => match key.code {
                KeyCode::Enter => app.submit_summary(),
                KeyCode::Esc => {
                    app.input_buffer.clear();
                    app.mode = Mode::Normal;
                }
                KeyCode::Char(c) => app.input_buffer.push(c),
                KeyCode::Backspace => {
                    app.input_buffer.pop();
                }
                _ => {}
            },
        },
        Event::Mouse(mouse) => {
            if app.mode != Mode::Normal {
                return;
            }
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let col = mouse.column;
                    let row = mouse.row;

                    // Check if click is in file list
                    let file_area = ui::file_list_area(frame_size);
                    if col >= file_area.x
                        && col < file_area.x + file_area.width
                        && row >= file_area.y + 1 // +1 for border
                        && row < file_area.y + file_area.height - 1
                    {
                        let index = (row - file_area.y - 1) as usize;
                        if index < app.files.len() {
                            app.select_file(index);
                        }
                        return;
                    }

                    // Check if click is in diff area
                    let diff_inner = ui::diff_area(frame_size);
                    if col >= diff_inner.x
                        && col < diff_inner.x + diff_inner.width
                        && row >= diff_inner.y
                        && row < diff_inner.y + diff_inner.height
                    {
                        let clicked_row = (row - diff_inner.y) as usize + app.diff_scroll;
                        // Map visible row to diff line index (accounting for comment lines)
                        if let Some(line_idx) = map_row_to_diff_line(app, clicked_row) {
                            app.commenting_line = Some(line_idx);
                            app.mode = Mode::Commenting;
                            app.input_buffer.clear();
                        }
                    }
                }
                MouseEventKind::ScrollUp => {
                    app.diff_scroll = app.diff_scroll.saturating_sub(3);
                }
                MouseEventKind::ScrollDown => {
                    app.diff_scroll += 3;
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Maps a visible row index to the actual diff line index,
/// accounting for inline comment lines that take up extra rows.
fn map_row_to_diff_line(app: &App, target_row: usize) -> Option<usize> {
    let diff = app.current_diff.as_ref()?;
    let file_comments = app
        .current_file
        .as_ref()
        .and_then(|f| app.comments.get(f));

    let all_lines: Vec<_> = diff.hunks.iter().flat_map(|h| h.lines.iter()).collect();
    let mut visual_row = 0;

    for (idx, _line) in all_lines.iter().enumerate() {
        if visual_row == target_row {
            return Some(idx);
        }
        visual_row += 1;
        // Account for comment lines below this diff line
        if let Some(comments) = file_comments {
            visual_row += comments.iter().filter(|c| c.line_index == idx).count();
        }
    }
    None
}

fn submit_review(app: &mut App) {
    let output = format_review(app);
    if output.is_empty() {
        return;
    }
    match arboard::Clipboard::new().and_then(|mut cb| cb.set_text(output)) {
        Ok(_) => {} // success — status bar will indicate
        Err(_) => {} // silently fail for now
    }
}

fn format_review(app: &App) -> String {
    let mut out = String::from("Code Review Feedback:\n");
    let mut has_content = false;

    for file in &app.files {
        if let Some(comments) = app.comments.get(&file.path) {
            if comments.is_empty() {
                continue;
            }
            let diff = app
                .current_file
                .as_ref()
                .filter(|f| *f == &file.path)
                .and_then(|_| app.current_diff.as_ref());

            for comment in comments {
                has_content = true;
                out.push('\n');

                // Get the line number and content
                let line_info = diff.and_then(|d| {
                    let all_lines: Vec<_> = d.hunks.iter().flat_map(|h| h.lines.iter()).collect();
                    all_lines.get(comment.line_index).map(|l| {
                        let line_no = l.new_line_no.or(l.old_line_no).unwrap_or(0);
                        (line_no, l.content.clone())
                    })
                });

                if let Some((line_no, content)) = line_info {
                    out.push_str(&format!("{}:{}\n", file.path, line_no));
                    out.push_str(&format!("> {}\n", content.trim()));
                } else {
                    out.push_str(&format!("{}:\n", file.path));
                }
                out.push_str(&format!("{}\n", comment.text));
            }
        }
    }

    if !app.summary.is_empty() {
        has_content = true;
        out.push_str(&format!("\nSummary: {}\n", app.summary));
    }

    if has_content {
        out
    } else {
        String::new()
    }
}
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 4: Manual test**

Run: `cargo run`
Expected: TUI launches with file list on right, diff on left. Clicking files loads diff. Clicking diff lines opens comment input. 'q' quits.

**Step 5: Commit**

```bash
git add src/ui.rs src/main.rs
git commit -m "feat: full TUI with layout, file list, diff view, inline comments, and clipboard submit"
```

---

### Task 6: Review Output Formatting Tests

**Files:**
- Modify: `src/main.rs` (extract format_review to its own module or add tests)

**Step 1: Add tests for format_review**

Since `format_review` is in main.rs, let's extract it. Create `src/review.rs`:

```rust
use crate::app::App;
use crate::diff::FileDiff;

pub fn format_review(app: &App) -> String {
    let mut out = String::from("Code Review Feedback:\n");
    let mut has_content = false;

    // We need to load diffs for all files that have comments
    for file in &app.files {
        if let Some(comments) = app.comments.get(&file.path) {
            if comments.is_empty() {
                continue;
            }

            // Load the diff for this file to get line content
            let diff = crate::git::get_file_diff(&file.path)
                .ok()
                .map(|raw| crate::diff::parse_diff(&raw));

            for comment in comments {
                has_content = true;
                out.push('\n');

                let line_info = diff.as_ref().and_then(|d| {
                    let all_lines: Vec<_> = d.hunks.iter().flat_map(|h| h.lines.iter()).collect();
                    all_lines.get(comment.line_index).map(|l| {
                        let line_no = l.new_line_no.or(l.old_line_no).unwrap_or(0);
                        (line_no, l.content.clone())
                    })
                });

                if let Some((line_no, content)) = line_info {
                    out.push_str(&format!("{}:{}\n", file.path, line_no));
                    out.push_str(&format!("> {}\n", content.trim()));
                } else {
                    out.push_str(&format!("{}:\n", file.path));
                }
                out.push_str(&format!("{}\n", comment.text));
            }
        }
    }

    if !app.summary.is_empty() {
        has_content = true;
        out.push_str(&format!("\nSummary: {}\n", app.summary));
    }

    if has_content {
        out
    } else {
        String::new()
    }
}

pub fn copy_to_clipboard(text: &str) -> Result<(), arboard::Error> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
```

**Step 2: Update main.rs to use review module**

Add `mod review;` and replace the inline `format_review` / `submit_review` with calls to `review::format_review` and `review::copy_to_clipboard`.

Update `submit_review` in main.rs:

```rust
fn submit_review(app: &App) {
    let output = review::format_review(app);
    if output.is_empty() {
        return;
    }
    let _ = review::copy_to_clipboard(&output);
}
```

**Step 3: Verify it compiles and tests pass**

Run: `cargo build && cargo test`
Expected: All pass.

**Step 4: Commit**

```bash
git add src/review.rs src/main.rs
git commit -m "feat: extract review formatting to module"
```

---

### Task 7: Polish & Edge Cases

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui.rs`
- Modify: `src/app.rs`

**Step 1: Add file navigation keyboard shortcuts**

In `handle_event` in `src/main.rs`, add to `Mode::Normal`:

```rust
KeyCode::Tab => {
    // Next file
    let next = app.file_list_state.selected().map_or(0, |i| {
        if i + 1 < app.files.len() { i + 1 } else { 0 }
    });
    app.select_file(next);
}
KeyCode::BackTab => {
    // Previous file
    let prev = app.file_list_state.selected().map_or(0, |i| {
        if i == 0 { app.files.len().saturating_sub(1) } else { i - 1 }
    });
    app.select_file(prev);
}
```

**Step 2: Add visual feedback on submit**

Add a `status_message` field to `App`:

```rust
pub status_message: Option<String>,
```

After successful clipboard copy, set it:

```rust
app.status_message = Some("Review copied to clipboard!".to_string());
```

Display it in the status bar when present.

**Step 3: Add refresh command**

Add `KeyCode::Char('r')` in Normal mode to re-run `git diff` and refresh the file list.

**Step 4: Verify everything works**

Run: `cargo build && cargo test`
Expected: All pass.

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: keyboard shortcuts, status feedback, refresh"
```

---

### Task 8: Final Integration & README

**Files:**
- Modify: `README.md`
- Modify: `Cargo.toml` (add metadata)

**Step 1: Update README with usage instructions**

Update `README.md` with:
- Installation: `cargo install --path .`
- Usage: run `revue` in any git repo
- Keybindings table
- Screenshot placeholder
- Tmux setup tip: `tmux split-window -h revue`

**Step 2: Add Cargo.toml metadata**

Add to `Cargo.toml`:

```toml
[package]
name = "revue"
version = "0.1.0"
edition = "2021"
description = "Mouse-heavy TUI code review sidecar for AI coding agents"
license = "MIT"
```

**Step 3: Final test**

Run: `cargo build --release && cargo test`
Expected: Clean build and all tests pass.

**Step 4: Commit and push**

```bash
git add -A
git commit -m "docs: update README with usage, add cargo metadata"
git push origin main
```

---

## Summary

| Task | Description | Key Files |
|------|-------------|-----------|
| 1 | Project scaffolding | `Cargo.toml`, `src/main.rs` |
| 2 | Git diff integration | `src/git.rs` |
| 3 | Unified diff parser | `src/diff.rs` |
| 4 | App state & data model | `src/app.rs` |
| 5 | Layout, file list, diff view, comments, clipboard | `src/ui.rs`, `src/main.rs` |
| 6 | Extract review module | `src/review.rs` |
| 7 | Polish: keybindings, status, refresh | all |
| 8 | README & metadata | `README.md`, `Cargo.toml` |
