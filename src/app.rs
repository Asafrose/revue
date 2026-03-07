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
