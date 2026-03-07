# revue

Mouse-heavy TUI code review sidecar for AI coding agents.

Track files changed against `main`, review diffs with inline comments, and copy structured feedback to clipboard — ready to paste into Claude Code, Codex, Gemini, or any AI coding assistant.

Built with Rust, [ratatui](https://ratatui.rs), and crossterm.

## Install

```bash
cargo install --path .
```

## Usage

Run `revue` in any git repository with changes against `main`:

```bash
revue
```

### Tmux sidecar setup

Split your terminal and run revue in the right pane:

```bash
tmux split-window -h revue
```

## Keybindings

| Key | Action |
|-----|--------|
| Click file | Select file, show diff |
| Click diff line | Add inline comment |
| `Tab` / `Shift-Tab` | Next / previous file |
| `j` / `k` | Scroll diff down / up |
| Scroll wheel | Scroll diff |
| `s` | Write review summary |
| `S` | Submit review (copy to clipboard) |
| `r` | Refresh file list |
| `q` | Quit |
| `Enter` | Save comment / summary |
| `Esc` | Cancel input |

## Clipboard output format

When you submit a review, revue copies structured feedback to your clipboard:

```
Code Review Feedback:

src/main.rs:42
> let buf = Vec::new();
This allocation inside the loop is unnecessary, move it outside.

src/lib.rs:15
> fn process(name: String) {
Consider using impl Into<String> instead of String here.

Summary: Overall looks good, but watch the performance in the hot path.
```

## License

MIT
