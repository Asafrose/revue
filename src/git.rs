use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Clone)]
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

pub fn get_file_diff(path: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["diff", "main", "--", path])
        .output()
        .context("Failed to run git diff for file")?;

    Ok(String::from_utf8(output.stdout)?)
}
