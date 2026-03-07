use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

pub(crate) fn parse_numstat(output: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let additions = parts[0].parse().unwrap_or(0);
        let deletions = parts[1].parse().unwrap_or(0);
        let path = parts[2].to_string();

        files.push(ChangedFile {
            path,
            change_type: ChangeType::Modified, // refined by parse_name_status
            additions,
            deletions,
        });
    }
    files
}

pub(crate) fn parse_name_status(output: &str, files: &mut Vec<ChangedFile>) {
    for line in output.lines() {
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
}

#[cfg(not(tarpaulin_include))]
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
    let mut files = parse_numstat(&stdout);

    // Get change types
    let type_output = Command::new("git")
        .args(["diff", "--name-status", "--diff-filter=ADMR", "main"])
        .output()
        .context("Failed to run git diff --name-status")?;

    let type_stdout = String::from_utf8(type_output.stdout)?;
    parse_name_status(&type_stdout, &mut files);

    Ok(files)
}

#[cfg(not(tarpaulin_include))]
pub fn get_file_diff(path: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "main", "--", path])
        .output()
        .context("Failed to run git diff for file")?;

    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_numstat tests ──────────────────────────────────────────

    #[test]
    fn parse_numstat_empty_input() {
        let files = parse_numstat("");
        assert!(files.is_empty());
    }

    #[test]
    fn parse_numstat_single_file() {
        let input = "10\t5\tpath/to/file.rs";
        let files = parse_numstat(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "path/to/file.rs");
        assert_eq!(files[0].additions, 10);
        assert_eq!(files[0].deletions, 5);
    }

    #[test]
    fn parse_numstat_multiple_files() {
        let input = "10\t5\tsrc/main.rs\n3\t1\tsrc/lib.rs\n20\t0\tREADME.md";
        let files = parse_numstat(input);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[1].path, "src/lib.rs");
        assert_eq!(files[1].additions, 3);
        assert_eq!(files[1].deletions, 1);
        assert_eq!(files[2].path, "README.md");
        assert_eq!(files[2].additions, 20);
        assert_eq!(files[2].deletions, 0);
    }

    #[test]
    fn parse_numstat_binary_file() {
        let input = "-\t-\timage.png";
        let files = parse_numstat(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "image.png");
        assert_eq!(files[0].additions, 0);
        assert_eq!(files[0].deletions, 0);
    }

    #[test]
    fn parse_numstat_malformed_line_skipped() {
        let input = "10\t5\tgood.rs\nmalformed_line\n8\t2\talso_good.rs";
        let files = parse_numstat(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "good.rs");
        assert_eq!(files[1].path, "also_good.rs");
    }

    // ── parse_name_status tests ──────────────────────────────────────

    #[test]
    fn parse_name_status_added_file() {
        let mut files = vec![ChangedFile {
            path: "new_file.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 10,
            deletions: 0,
        }];
        parse_name_status("A\tnew_file.rs", &mut files);
        assert_eq!(files[0].change_type, ChangeType::Added);
    }

    #[test]
    fn parse_name_status_modified_file() {
        let mut files = vec![ChangedFile {
            path: "existing.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 5,
            deletions: 3,
        }];
        parse_name_status("M\texisting.rs", &mut files);
        assert_eq!(files[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn parse_name_status_deleted_file() {
        let mut files = vec![ChangedFile {
            path: "old.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 0,
            deletions: 50,
        }];
        parse_name_status("D\told.rs", &mut files);
        assert_eq!(files[0].change_type, ChangeType::Deleted);
    }

    #[test]
    fn parse_name_status_renamed_file() {
        let mut files = vec![ChangedFile {
            path: "new_name.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 2,
            deletions: 1,
        }];
        parse_name_status("R100\told_name.rs\tnew_name.rs", &mut files);
        assert_eq!(files[0].change_type, ChangeType::Renamed);
    }

    #[test]
    fn parse_name_status_missing_file_ignored() {
        let mut files = vec![ChangedFile {
            path: "exists.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 1,
            deletions: 1,
        }];
        parse_name_status("A\tnot_in_vec.rs", &mut files);
        // The existing file should remain unchanged
        assert_eq!(files[0].change_type, ChangeType::Modified);
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn parse_name_status_malformed_line_skipped() {
        let mut files = vec![ChangedFile {
            path: "file.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 1,
            deletions: 0,
        }];
        // Line with no tab separator should be skipped (hits the continue on line 45)
        parse_name_status("A\tfile.rs\nmalformed_no_tab\nD\tfile.rs", &mut files);
        // The last status wins: Deleted
        assert_eq!(files[0].change_type, ChangeType::Deleted);
    }

    #[test]
    fn parse_name_status_empty_input() {
        let mut files = vec![ChangedFile {
            path: "file.rs".to_string(),
            change_type: ChangeType::Modified,
            additions: 1,
            deletions: 0,
        }];
        parse_name_status("", &mut files);
        // No changes should have been made
        assert_eq!(files[0].change_type, ChangeType::Modified);
    }
}
