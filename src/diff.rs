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
            if !current_lines.is_empty() {
                hunks.push(Hunk {
                    header: current_header.clone(),
                    lines: std::mem::take(&mut current_lines),
                });
            }
            current_header = line.to_string();

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
            header: current_header,
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
}
