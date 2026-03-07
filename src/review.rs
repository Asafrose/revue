use crate::app::App;
use crate::diff::{self, FileDiff};
use crate::git;

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

pub fn copy_to_clipboard(text: &str) -> Result<(), arboard::Error> {
    let mut cb = arboard::Clipboard::new()?;
    cb.set_text(text)?;
    Ok(())
}
