# Revue v1 Design

## What it is

A Rust TUI app (ratatui + crossterm) designed to run as a tmux sidecar (right pane) alongside any AI coding agent. It shows a GitHub-style code review interface for files changed against `main`, lets you leave inline comments on specific lines and an overall summary, then copies structured feedback to clipboard.

Agent-agnostic: works with Claude Code, Codex, Gemini, OpenCode, or any tool that accepts text input.

## Layout

```
┌──────────────────────────┬──────────────┐
│                          │  File List   │
│     Diff View            │  ─────────── │
│     (main panel)         │  M src/main  │
│                          │  A src/lib   │
│                          │  D old.rs    │
│                          │              │
├──────────────────────────┴──────────────┤
│  Status bar / keybindings / summary     │
└─────────────────────────────────────────┘
```

- **Right sidebar** — file list with change type indicators (A/M/D/R), +/- line counts, reviewed checkmarks. Right-sided because revue is typically docked to the right in tmux.
- **Main panel** — unified diff view with syntax highlighting, line numbers, clickable lines for adding inline comments.
- **Bottom bar** — status info, keybinding hints, summary input.

## Core flow

1. On launch, run `git diff main...HEAD` (or `main` vs working tree) to get changed files.
2. Render file list in right sidebar.
3. User clicks a file to load its diff in the main panel.
4. User clicks a diff line to open an inline comment input.
5. User writes comments across files, optionally adds an overall summary.
6. User hits "submit" — structured review copied to clipboard.
7. User pastes into their AI coding agent.

## Review interaction

- **Inline comments:** click a line in the diff, type a comment. Comment appears below the line in the diff view.
- **Summary:** overall review message, entered via the bottom bar or a dedicated input.
- **Submit:** bundles all comments + summary into clipboard.
- **Mouse-first:** all core actions are mouse-driven. Keyboard shortcuts as secondary.

## Clipboard output format

```
Code Review Feedback:

src/main.rs:42
> let buf = Vec::new();
This allocation inside the loop is unnecessary, move it outside.

src/lib.rs:15
> fn process(name: String) {
Consider using `impl Into<String>` instead of `String` here.

Summary: Overall looks good, but watch the performance in the hot path.
```

## Technical decisions

- **Framework:** ratatui + crossterm
- **Git integration:** shell out to `git` CLI (simpler than libgit2, sufficient for our needs)
- **Clipboard:** `arboard` crate (cross-platform)
- **Syntax highlighting:** `syntect` crate
- **Diff parsing:** parse unified diff output directly (simple format)

## Non-goals

- File editing (never — this is a review tool, not an editor)

## Future enhancements (tracked as GitHub issues)

- [#1](https://github.com/Asafrose/revue/issues/1) Side-by-side diff view
- [#2](https://github.com/Asafrose/revue/issues/2) Direct IPC to AI coding agents
- [#3](https://github.com/Asafrose/revue/issues/3) Multi-commit review / commit selection
- [#4](https://github.com/Asafrose/revue/issues/4) Conflict resolution view
