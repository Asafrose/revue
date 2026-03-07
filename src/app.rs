use crate::diff::FileDiff;
use crate::git::ChangedFile;
use ratatui::widgets::ListState;
use std::collections::HashMap;

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
    pub input_buffer: String,
    pub diff_scroll: usize,
    pub diff_hscroll: usize,
    pub commenting_line: Option<usize>,
    pub should_quit: bool,
    pub status_message: Option<String>,
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
            diff_hscroll: 0,
            commenting_line: None,
            should_quit: false,
            status_message: None,
        }
    }

    pub fn selected_file(&self) -> Option<&ChangedFile> {
        self.file_list_state.selected().and_then(|i| self.files.get(i))
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
                header: "@@ -1,3 +1,4 @@".to_string(),
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
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.diff_scroll, 0);
        assert!(app.current_diff.is_none());
        assert!(app.current_file.is_none());
        assert!(app.commenting_line.is_none());
        assert!(!app.should_quit);
        assert!(app.status_message.is_none());
    }

    // ── App::selected_file ─────────────────────────────────────────────

    #[test]
    fn selected_file_with_selection_returns_correct_file() {
        let app = App::new(vec![make_test_file("a.rs"), make_test_file("b.rs")]);
        let file = app.selected_file().unwrap();
        assert_eq!(file.path, "a.rs");
    }

    #[test]
    fn selected_file_empty_files_returns_none() {
        let app = App::new(vec![]);
        assert!(app.selected_file().is_none());
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
        app.input_buffer = "looks good".to_string();
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
        app.input_buffer = String::new();
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_no_commenting_line_no_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = None;
        app.input_buffer = "some text".to_string();
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_no_current_file_no_comment() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = None;
        app.commenting_line = Some(5);
        app.input_buffer = "some text".to_string();
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.comments.is_empty());
    }

    #[test]
    fn submit_comment_clears_state() {
        let mut app = App::new(vec![make_test_file("a.rs")]);
        app.current_file = Some("a.rs".to_string());
        app.commenting_line = Some(5);
        app.input_buffer = "text".to_string();
        app.mode = Mode::Commenting;

        app.submit_comment();

        assert!(app.input_buffer.is_empty());
        assert!(app.commenting_line.is_none());
        assert_eq!(app.mode, Mode::Normal);
    }

    // ── App::submit_summary ────────────────────────────────────────────

    #[test]
    fn submit_summary_sets_summary() {
        let mut app = App::new(vec![]);
        app.input_buffer = "Overall looks great".to_string();
        app.mode = Mode::Summary;

        app.submit_summary();

        assert_eq!(app.summary, "Overall looks great");
    }

    #[test]
    fn submit_summary_clears_input_buffer() {
        let mut app = App::new(vec![]);
        app.input_buffer = "summary text".to_string();
        app.mode = Mode::Summary;

        app.submit_summary();

        assert!(app.input_buffer.is_empty());
    }

    #[test]
    fn submit_summary_returns_to_normal_mode() {
        let mut app = App::new(vec![]);
        app.input_buffer = "text".to_string();
        app.mode = Mode::Summary;

        app.submit_summary();

        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn submit_summary_empty_buffer_sets_empty_summary() {
        let mut app = App::new(vec![]);
        app.input_buffer = String::new();
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
                ReviewComment { line_index: 1, text: "fix this".to_string() },
                ReviewComment { line_index: 5, text: "and this".to_string() },
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
            vec![ReviewComment { line_index: 1, text: "one".to_string() }],
        );
        app.comments.insert(
            "b.rs".to_string(),
            vec![
                ReviewComment { line_index: 1, text: "two".to_string() },
                ReviewComment { line_index: 2, text: "three".to_string() },
                ReviewComment { line_index: 3, text: "four".to_string() },
            ],
        );
        assert_eq!(app.file_comment_count("a.rs"), 1);
        assert_eq!(app.file_comment_count("b.rs"), 3);
        assert_eq!(app.file_comment_count("c.rs"), 0);
    }
}
