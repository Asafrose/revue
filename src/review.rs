use crate::app::App;
use crate::diff::{self, FileDiff};
use crate::git;

#[cfg(not(tarpaulin_include))]
pub fn format_review(app: &App) -> String {
    format_review_with(app, |path| {
        git::get_file_diff(path).ok().map(|raw| diff::parse_diff(&raw))
    })
}

pub(crate) fn format_review_with<F>(app: &App, get_diff: F) -> String
where
    F: Fn(&str) -> Option<FileDiff>,
{
    let mut out = String::from("Code Review Feedback:\n");
    let mut has_content = false;

    for file in &app.files {
        if let Some(comments) = app.comments.get(&file.path) {
            if comments.is_empty() {
                continue;
            }

            let diff = get_diff(&file.path);

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

#[cfg(not(tarpaulin_include))]
pub fn copy_to_clipboard(text: &str) -> Result<(), arboard::Error> {
    let mut cb = arboard::Clipboard::new()?;
    cb.set_text(text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, ReviewComment};
    use crate::diff::{DiffLine, FileDiff, Hunk, LineType};
    use crate::git::{ChangeType, ChangedFile};

    fn make_file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            change_type: ChangeType::Modified,
            additions: 1,
            deletions: 1,
        }
    }

    fn make_diff() -> FileDiff {
        FileDiff {
            hunks: vec![Hunk {
                header: "@@ -1,3 +1,3 @@".to_string(),
                lines: vec![
                    DiffLine {
                        line_type: LineType::Context,
                        content: "context".to_string(),
                        old_line_no: Some(1),
                        new_line_no: Some(1),
                    },
                    DiffLine {
                        line_type: LineType::Deletion,
                        content: "old code".to_string(),
                        old_line_no: Some(2),
                        new_line_no: None,
                    },
                    DiffLine {
                        line_type: LineType::Addition,
                        content: "new code".to_string(),
                        old_line_no: None,
                        new_line_no: Some(2),
                    },
                ],
            }],
        }
    }

    // ── 1. No comments, no summary → empty string ───────────────────

    #[test]
    fn no_comments_no_summary_returns_empty() {
        let app = App::new(vec![make_file("src/main.rs")]);
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert_eq!(result, "");
    }

    // ── 2. Only summary, no comments → header + summary ─────────────

    #[test]
    fn only_summary_no_comments() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.summary = "Looks good overall".to_string();
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert_eq!(result, "Code Review Feedback:\n\nSummary: Looks good overall\n");
    }

    // ── 3. One file, one comment, diff available ─────────────────────

    #[test]
    fn one_file_one_comment_with_diff() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 0,
                text: "This context line needs work".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert!(result.starts_with("Code Review Feedback:\n"));
        assert!(result.contains("src/main.rs:1\n"));
        assert!(result.contains("> context\n"));
        assert!(result.contains("This context line needs work\n"));
    }

    #[test]
    fn one_file_one_comment_on_deletion_line() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 1, // Deletion line: old_line_no=Some(2), new_line_no=None
                text: "Why was this removed?".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        // Falls back to old_line_no since new_line_no is None
        assert!(result.contains("src/main.rs:2\n"));
        assert!(result.contains("> old code\n"));
        assert!(result.contains("Why was this removed?\n"));
    }

    #[test]
    fn one_file_one_comment_on_addition_line() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 2, // Addition line: new_line_no=Some(2)
                text: "Nice change".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert!(result.contains("src/main.rs:2\n"));
        assert!(result.contains("> new code\n"));
        assert!(result.contains("Nice change\n"));
    }

    // ── 4. One file, multiple comments ───────────────────────────────

    #[test]
    fn one_file_multiple_comments() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![
                ReviewComment {
                    line_index: 0,
                    text: "First comment".to_string(),
                },
                ReviewComment {
                    line_index: 2,
                    text: "Second comment".to_string(),
                },
            ],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert!(result.contains("First comment"));
        assert!(result.contains("Second comment"));
        // Both should reference the file
        let count = result.matches("src/main.rs:").count();
        assert_eq!(count, 2);
    }

    // ── 5. Multiple files with comments ──────────────────────────────

    #[test]
    fn multiple_files_with_comments() {
        let mut app = App::new(vec![
            make_file("src/main.rs"),
            make_file("src/lib.rs"),
        ]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 0,
                text: "Comment on main".to_string(),
            }],
        );
        app.comments.insert(
            "src/lib.rs".to_string(),
            vec![ReviewComment {
                line_index: 1,
                text: "Comment on lib".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert!(result.contains("src/main.rs:"));
        assert!(result.contains("Comment on main"));
        assert!(result.contains("src/lib.rs:"));
        assert!(result.contains("Comment on lib"));
    }

    // ── 6. Diff unavailable (get_diff returns None) ──────────────────

    #[test]
    fn diff_unavailable_uses_file_only_format() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 0,
                text: "Cannot see the code".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| None);
        assert!(result.contains("src/main.rs:\n"));
        assert!(result.contains("Cannot see the code\n"));
        // Should NOT contain a line number after the file path
        assert!(!result.contains("src/main.rs:0"));
        assert!(!result.contains("src/main.rs:1"));
        // Should NOT contain quoted code
        assert!(!result.contains("> "));
    }

    // ── 7. Comments + summary together ───────────────────────────────

    #[test]
    fn comments_and_summary_together() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 0,
                text: "Fix this".to_string(),
            }],
        );
        app.summary = "Needs another pass".to_string();
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert!(result.contains("Fix this"));
        assert!(result.contains("Summary: Needs another pass"));
        // Summary should appear at the end
        let summary_pos = result.find("Summary:").unwrap();
        let comment_pos = result.find("Fix this").unwrap();
        assert!(summary_pos > comment_pos);
    }

    // ── 8. File with empty comments vec → skipped ────────────────────

    #[test]
    fn file_with_empty_comments_vec_is_skipped() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments
            .insert("src/main.rs".to_string(), vec![]);
        let result = format_review_with(&app, |_path| Some(make_diff()));
        assert_eq!(result, "");
    }

    // ── 9. line_index beyond diff lines → fallback format ────────────

    #[test]
    fn line_index_beyond_diff_lines_falls_back() {
        let mut app = App::new(vec![make_file("src/main.rs")]);
        app.comments.insert(
            "src/main.rs".to_string(),
            vec![ReviewComment {
                line_index: 999, // way beyond the 3 lines in make_diff()
                text: "Out of range comment".to_string(),
            }],
        );
        let result = format_review_with(&app, |_path| Some(make_diff()));
        // Should fall back to "file:" format without line number
        assert!(result.contains("src/main.rs:\n"));
        assert!(result.contains("Out of range comment\n"));
        assert!(!result.contains("> "));
    }
}
