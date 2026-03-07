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
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub hunks: Vec<Hunk>,
}

pub fn parse_diff(raw: &str) -> FileDiff {
    let mut hunks = Vec::new();
    let mut current_lines: Vec<DiffLine> = Vec::new();
    let mut old_line: usize = 0;
    let mut new_line: usize = 0;

    for line in raw.lines() {
        if line.starts_with("@@") {
            if !current_lines.is_empty() {
                hunks.push(Hunk {
                    lines: std::mem::take(&mut current_lines),
                });
            }

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

    if !current_lines.is_empty() {
        hunks.push(Hunk {
            lines: current_lines,
        });
    }

    FileDiff { hunks }
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
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
        assert_eq!(hunk.lines[1].old_line_no, Some(10));
        assert_eq!(hunk.lines[1].new_line_no, Some(10));
        assert_eq!(hunk.lines[3].old_line_no, Some(12));
        assert_eq!(hunk.lines[3].new_line_no, None);
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

    #[test]
    fn test_multi_hunk_diff() {
        let raw = r#"diff --git a/file.rs b/file.rs
--- a/file.rs
+++ b/file.rs
@@ -1,4 +1,4 @@
 line1
-line2
+LINE2
 line3
@@ -20,4 +20,4 @@
 line20
-line21
+LINE21
 line22
"#;
        let diff = parse_diff(raw);
        assert_eq!(diff.hunks.len(), 2);

        // First hunk starts at old=1, new=1
        let h0 = &diff.hunks[0];
        assert_eq!(h0.lines[0].line_type, LineType::HunkHeader);
        assert_eq!(h0.lines[1].old_line_no, Some(1));
        assert_eq!(h0.lines[1].new_line_no, Some(1));
        // Deletion at old_line 2
        assert_eq!(h0.lines[2].line_type, LineType::Deletion);
        assert_eq!(h0.lines[2].old_line_no, Some(2));
        // Addition at new_line 2
        assert_eq!(h0.lines[3].line_type, LineType::Addition);
        assert_eq!(h0.lines[3].new_line_no, Some(2));

        // Second hunk starts at old=20, new=20
        let h1 = &diff.hunks[1];
        assert_eq!(h1.lines[0].line_type, LineType::HunkHeader);
        assert_eq!(h1.lines[1].old_line_no, Some(20));
        assert_eq!(h1.lines[1].new_line_no, Some(20));
        assert_eq!(h1.lines[2].line_type, LineType::Deletion);
        assert_eq!(h1.lines[2].old_line_no, Some(21));
        assert_eq!(h1.lines[3].line_type, LineType::Addition);
        assert_eq!(h1.lines[3].new_line_no, Some(21));
    }

    #[test]
    fn test_delete_only_diff() {
        let raw = r#"diff --git a/file.rs b/file.rs
--- a/file.rs
+++ b/file.rs
@@ -1,4 +1,2 @@
 keep
-remove1
-remove2
 keep2
"#;
        let diff = parse_diff(raw);
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        let deletions: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.line_type == LineType::Deletion)
            .collect();
        assert_eq!(deletions.len(), 2);
        let additions: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.line_type == LineType::Addition)
            .collect();
        assert_eq!(additions.len(), 0);
        for d in &deletions {
            assert!(d.old_line_no.is_some());
            assert!(d.new_line_no.is_none());
        }
    }

    #[test]
    fn test_add_only_diff() {
        let raw = r#"diff --git a/file.rs b/file.rs
--- a/file.rs
+++ b/file.rs
@@ -1,2 +1,4 @@
 keep
+added1
+added2
 keep2
"#;
        let diff = parse_diff(raw);
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        let additions: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.line_type == LineType::Addition)
            .collect();
        assert_eq!(additions.len(), 2);
        let deletions: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.line_type == LineType::Deletion)
            .collect();
        assert_eq!(deletions.len(), 0);
        for a in &additions {
            assert!(a.old_line_no.is_none());
            assert!(a.new_line_no.is_some());
        }
    }

    #[test]
    fn test_hunk_header_without_context() {
        // No function name after @@
        let result = parse_hunk_header("@@ -1,3 +1,3 @@");
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn test_hunk_header_single_line_no_comma() {
        let result = parse_hunk_header("@@ -1 +1 @@");
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn test_parse_hunk_header_invalid_input() {
        // Missing @@ prefix
        assert_eq!(parse_hunk_header("-1,3 +1,3"), None);
        // Missing +/- markers
        assert_eq!(parse_hunk_header("@@ 1,3 1,3 @@"), None);
        // Complete garbage
        assert_eq!(parse_hunk_header("hello world"), None);
        // Empty string
        assert_eq!(parse_hunk_header(""), None);
        // Only @@
        assert_eq!(parse_hunk_header("@@"), None);
        // @@ with space but no ranges
        assert_eq!(parse_hunk_header("@@ @@"), None);
    }

    #[test]
    fn test_no_newline_at_end_of_file_marker() {
        let raw = r#"diff --git a/file.rs b/file.rs
--- a/file.rs
+++ b/file.rs
@@ -1,3 +1,3 @@
 line1
-old_last
+new_last
\ No newline at end of file
"#;
        let diff = parse_diff(raw);
        assert_eq!(diff.hunks.len(), 1);
        let hunk = &diff.hunks[0];
        // The "\ No newline..." line starts with '\' which falls through
        // to the else branch and is not included as a diff line.
        // We should have: HunkHeader, Context(line1), Deletion(old_last), Addition(new_last)
        let types: Vec<_> = hunk.lines.iter().map(|l| &l.line_type).collect();
        assert_eq!(
            types,
            vec![
                &LineType::HunkHeader,
                &LineType::Context,
                &LineType::Deletion,
                &LineType::Addition,
            ]
        );
    }
}
