use crate::diff::FileDiff;
use crate::git::{ChangedFile, CommitInfo};
use git2::Oid;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::time::Instant;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use tui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone)]
pub struct ReviewComment {
    pub line_index: usize,
    pub text: String,
}

pub struct App {
    pub files: Vec<ChangedFile>,
    pub file_list_state: ListState,
    pub current_diff: Option<FileDiff>,
    pub current_file: Option<String>,
    pub comments: HashMap<String, Vec<ReviewComment>>,
    pub summary: String,
    pub mode: Mode,
    pub textarea: Option<TextArea<'static>>,
    pub diff_scroll: usize,
    pub diff_hscroll: usize,
    pub commenting_line: Option<usize>,
    /// When editing an existing comment, tracks (file_path, index in comments vec).
    pub editing_comment: Option<(String, usize)>,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub syntax_set: SyntaxSet,
    pub theme_set: ThemeSet,
    pub cursor_blink_start: Instant,
    /// Commits between main and HEAD, newest first.
    pub commits: Vec<CommitInfo>,
    /// Which commits are selected (by index into `commits`). All selected by default.
    pub selected_commits: Vec<bool>,
    pub commit_list_state: ListState,
    /// The OID of the main branch commit (base for diffs).
    pub main_oid: Option<Oid>,
    pub file_scroll: usize,
    pub sidebar_width: u16,
    pub dragging_sidebar: bool,
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
            textarea: None,
            diff_scroll: 0,
            diff_hscroll: 0,
            commenting_line: None,
            editing_comment: None,
            should_quit: false,
            status_message: None,
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            cursor_blink_start: Instant::now(),
            commits: Vec::new(),
            selected_commits: Vec::new(),
            commit_list_state: ListState::default(),
            main_oid: None,
            file_scroll: 0,
            sidebar_width: crate::ui::DEFAULT_SIDEBAR_WIDTH,
            dragging_sidebar: false,
        }
    }

    /// Select a file by index and set its diff data.
    /// The caller is responsible for loading the diff.
    pub fn select_file_with_diff(&mut self, index: usize, diff: Option<FileDiff>) {
        if index < self.files.len() {
            self.file_list_state.select(Some(index));
            self.current_file = Some(self.files[index].path.clone());
            self.current_diff = diff;
            self.diff_scroll = 0;
            self.diff_hscroll = 0;
        }
    }

    pub fn select_file(&mut self, index: usize) {
        if index < self.files.len() {
            let path = self.files[index].path.clone();
            let diff = crate::git::get_file_diff(&path)
                .ok()
                .map(|raw| crate::diff::parse_diff(&raw));
            self.select_file_with_diff(index, diff);
        }
    }

    pub fn input_text(&self) -> String {
        self.textarea
            .as_ref()
            .map(|ta| ta.lines().join("\n"))
            .unwrap_or_default()
    }

    pub fn start_input(&mut self, initial_text: &str) {
        let lines = if initial_text.is_empty() {
            vec!["".to_string()]
        } else {
            initial_text.lines().map(|l| l.to_string()).collect()
        };
        let mut textarea = TextArea::new(lines);
        textarea.move_cursor(CursorMove::Bottom);
        textarea.move_cursor(CursorMove::End);
        self.textarea = Some(textarea);
    }

    pub fn clear_input(&mut self) {
        self.textarea = None;
    }

    pub fn submit_comment(&mut self) {
        let text = self.input_text();
        if let Some((ref file, idx)) = self.editing_comment {
            // Editing existing comment
            if text.is_empty() {
                // Empty text deletes the comment
                if let Some(comments) = self.comments.get_mut(file) {
                    if idx < comments.len() {
                        comments.remove(idx);
                    }
                    if comments.is_empty() {
                        self.comments.remove(file);
                    }
                }
            } else if let Some(comments) = self.comments.get_mut(file) {
                if idx < comments.len() {
                    comments[idx].text = text;
                }
            }
        } else if let (Some(line_idx), Some(file)) = (self.commenting_line, &self.current_file) {
            // New comment
            if !text.is_empty() {
                let comment = ReviewComment {
                    line_index: line_idx,
                    text,
                };
                self.comments.entry(file.clone()).or_default().push(comment);
            }
        }
        self.clear_input();
        self.commenting_line = None;
        self.editing_comment = None;
        self.mode = Mode::Normal;
    }

    pub fn delete_comment(&mut self) {
        if let Some((ref file, idx)) = self.editing_comment {
            if let Some(comments) = self.comments.get_mut(file) {
                if idx < comments.len() {
                    comments.remove(idx);
                }
                if comments.is_empty() {
                    self.comments.remove(file);
                }
            }
        }
        self.clear_input();
        self.commenting_line = None;
        self.editing_comment = None;
        self.mode = Mode::Normal;
    }

    pub fn submit_summary(&mut self) {
        self.summary = self.input_text();
        self.clear_input();
        self.mode = Mode::Normal;
    }

    pub fn file_comment_count(&self, path: &str) -> usize {
        self.comments.get(path).map_or(0, |c| c.len())
    }

    pub fn set_commits(&mut self, commits: Vec<CommitInfo>, main_oid: Oid) {
        let count = commits.len();
        self.commits = commits;
        self.selected_commits = vec![true; count];
        self.main_oid = Some(main_oid);
        if count > 0 {
            self.commit_list_state.select(Some(0));
        }
    }

    pub fn toggle_commit(&mut self, index: usize) {
        if index < self.selected_commits.len() {
            self.selected_commits[index] = !self.selected_commits[index];
        }
    }

    /// Returns the "from" OID for diffing based on selected commits.
    /// This is the parent of the oldest selected commit, or main if the oldest is selected.
    /// Returns None if no commits are selected (falls back to main diff).
    pub fn diff_from_oid(&self) -> Option<Oid> {
        if self.commits.is_empty() {
            return self.main_oid;
        }
        // If no commits selected, diff everything against main
        if !self.selected_commits.iter().any(|&s| s) {
            return self.main_oid;
        }
        // Find the oldest selected commit (last in vec since newest-first)
        let oldest_idx = self
            .selected_commits
            .iter()
            .enumerate()
            .rev()
            .find(|(_, &s)| s)
            .map(|(i, _)| i)?;
        // The "from" is the parent of the oldest selected commit,
        // which is the commit just after it in our list (older), or main_oid
        if oldest_idx + 1 < self.commits.len() {
            Some(self.commits[oldest_idx + 1].id)
        } else {
            self.main_oid
        }
    }

    /// Returns the "to" OID for diffing. None means diff to working directory.
    /// If the newest selected commit is not the first one, we diff up to that commit.
    pub fn diff_to_oid(&self) -> Option<Oid> {
        if self.commits.is_empty() {
            return None; // working dir
        }
        if !self.selected_commits.iter().any(|&s| s) {
            return None; // working dir
        }
        // Find newest selected commit
        let newest_idx = self.selected_commits.iter().position(|&s| s)?;
        if newest_idx == 0 {
            // Newest commit selected — include working dir changes
            None
        } else {
            Some(self.commits[newest_idx].id)
        }
    }

    /// Reload file list based on current commit selection.
    pub fn reload_files_for_selection(&mut self) {
        let from = self.diff_from_oid();
        let to = self.diff_to_oid();
        if let Some(from) = from {
            if let Ok(files) = crate::git::get_changed_files_for_range(from, to) {
                self.files = files;
                self.file_list_state = ListState::default();
                if !self.files.is_empty() {
                    self.file_list_state.select(Some(0));
                }
                self.current_diff = None;
                self.current_file = None;
                self.file_scroll = 0;
            }
        }
    }

    /// Select a file using the current commit range for diff.
    pub fn select_file_for_range(&mut self, index: usize) {
        if index < self.files.len() {
            let path = self.files[index].path.clone();
            let from = self.diff_from_oid();
            let to = self.diff_to_oid();
            let diff = from.and_then(|f| {
                crate::git::get_file_diff_for_range(&path, f, to)
                    .ok()
                    .map(|raw| crate::diff::parse_diff(&raw))
            });
            self.file_list_state.select(Some(index));
            self.current_file = Some(path);
            self.current_diff = diff;
            self.diff_scroll = 0;
            self.diff_hscroll = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffLine, FileDiff, Hunk, LineType};
    use crate::git::{ChangeType, ChangedFile};

    fn make_test_file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            change_type: ChangeType::Modified,
            additions: 5,
            deletions: 3,
        }
    }

    fn make_test_diff() -> FileDiff {
        FileDiff {
            hunks: vec![Hunk {
                lines: vec![
                    DiffLine {
                        line_type: LineType::Context,
                        content: "line 1".to_string(),
                        old_line_no: Some(1),
                        new_line_no: Some(1),
                    },
                    DiffLine {
                        line_type: LineType::Deletion,
                        content: "old line".to_string(),
                        old_line_no: Some(2),
                        new_line_no: None,
                    },
                    DiffLine {
                        line_type: LineType::Addition,
                        content: "new line".to_string(),
                        old_line_no: None,
                        new_line_no: Some(2),
                    },
                ],
            }],
        }
    }

    // ── App::new ───────────────────────────────────────────────────────

    #[test]
    fn new_empty_files_selection_is_none() {
        let app = App::new(vec![]);
        assert!(app.file_list_state.selected().is_none());
    }

    #[test]
    fn new_empty_files_mode_is_normal() {
        let app = App::new(vec![]);
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn new_non_empty_files_selects_first() {
        let app = App::new(vec![make_test_file("a.rs")]);
        assert_eq!(app.file_list_state.selected(), Some(0));
    }

    #[test]
    fn new_initializes_all_fields() {
        let files = vec![make_test_file("a.rs"), make_test_file("b.rs")];
        let app = App::new(files.clone());
        assert_eq!(app.files.len(), 2);
        assert!(app.comments.is_empty());
        assert!(app.summary.is_empty());
        assert!(app.textarea.is_none());
        assert_eq!(app.diff_scroll, 0);
        assert!(app.current_diff.is_none());
        assert!(app.current_file.is_none());
        assert!(app.commenting_line.is_none());
        assert!(!app.should_quit);
        assert!(app.status_message.is_none());
    }

    // ── App::select_file_with_diff ─────────────────────────────────────

    #[test]
    fn select_file_with_diff_valid_index() {
        let mut app = App::new(vec![make_test_file("a.rs"), make_test_file("b.rs")]);
        let diff = make_test_diff();
        app.select_file_with_diff(1, Some(diff));
        assert_eq!(app.current_file.as_deref(), Some("b.rs"));
        assert!(app.current_diff.is_some());
        assert_eq!(app.file_list_state.selected(), Some(1));
    }

    #[test]
    fn select_file_with_diff_out_of_bounds_no_change() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.select_file_with_diff(5, Some(make_test_diff()));
        // Should remain unchanged
        assert!(app.current_file.is_none());
        assert!(app.current_diff.is_none());
        assert_eq!(app.file_list_state.selected(), Some(0));
    }

    #[test]
    fn select_file_with_diff_resets_scroll() {
        let mut app = App::new(vec![make_test_file("a.rs"), make_test_file("b.rs")]);
        app.diff_scroll = 10;
        app.select_file_with_diff(1, Some(make_test_diff()));
        assert_eq!(app.diff_scroll, 0);
    }

    // ── App::submit_comment ────────────────────────────────────────────

    #[test]
    fn submit_comment_valid_adds_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = Some(5);
        app.start_input("looks good");
        app.mode = Mode::Commenting;

        app.submit_comment();

        let comments = app.comments.get("a.rs").unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].line_index, 5);
        assert_eq!(comments[0].text, "looks good");
    }

    #[test]
    fn submit_comment_empty_buffer_no_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = Some(5);
        app.start_input("");
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_no_commenting_line_no_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = None;
        app.start_input("some text");
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_no_current_file_no_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = None;
        app.commenting_line = Some(5);
        app.start_input("some text");
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_clears_state() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = Some(5);
        app.start_input("text");
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.textarea.is_none());
        assert!(app.commenting_line.is_none());
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── App::submit_summary ────────────────────────────────────────────

    #[test]
    fn submit_summary_sets_summary() {
        let mut app = App::new(vec![]);
        app.start_input("Overall looks great");
        app.mode = Mode::Summary;

        app.submit_summary();

        assert_eq!(app.summary, "Overall looks great");
    }

    #[test]
    fn submit_summary_clears_textarea() {
        let mut app = App::new(vec![]);
        app.start_input("summary text");
        app.mode = Mode::Summary;

        app.submit_summary();

        assert!(app.textarea.is_none());
    }

    #[test]
    fn submit_summary_returns_to_normal_mode() {
        let mut app = App::new(vec![]);
        app.start_input("text");
        app.mode = Mode::Summary;

        app.submit_summary();

        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn submit_summary_empty_buffer_sets_empty_summary() {
        let mut app = App::new(vec![]);
        app.start_input("");
        app.mode = Mode::Summary;

        app.submit_summary();

        assert_eq!(app.summary, "");
    }

    // ── App::file_comment_count ────────────────────────────────────────

    #[test]
    fn file_comment_count_no_comments_returns_zero() {
        let app = App::new(vec![make_test_file("a.rs")]);
        assert_eq!(app.file_comment_count("a.rs"), 0);
    }

    #[test]
    fn file_comment_count_with_comments_returns_correct_count() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.comments.insert(
            "a.rs".to_string(),
            vec![
                ReviewComment {
                    line_index: 1,
                    text: "fix this".to_string(),
                },
                ReviewComment {
                    line_index: 5,
                    text: "and this".to_string(),
                },
            ],
        );
        assert_eq!(app.file_comment_count("a.rs"), 2);
    }

    #[test]
    fn file_comment_count_multiple_files() {
        let mut app = App::new(vec![
            make_test_file("a.rs"),
            make_test_file("b.rs"),
            make_test_file("c.rs"),
        ]);
        app.comments.insert(
            "a.rs".to_string(),
            vec![ReviewComment {
                line_index: 1,
                text: "one".to_string(),
            }],
        );
        app.comments.insert(
            "b.rs".to_string(),
            vec![
                ReviewComment {
                    line_index: 1,
                    text: "two".to_string(),
                },
                ReviewComment {
                    line_index: 2,
                    text: "three".to_string(),
                },
                ReviewComment {
                    line_index: 3,
                    text: "four".to_string(),
                },
            ],
        );
        assert_eq!(app.file_comment_count("a.rs"), 1);
        assert_eq!(app.file_comment_count("b.rs"), 3);
        assert_eq!(app.file_comment_count("c.rs"), 0);
    }
}
