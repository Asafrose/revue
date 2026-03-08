use anyhow::{Context, Result};
use git2::{Delta, Diff, DiffOptions, Patch, Repository};

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

fn open_repo() -> Result<Repository> {
    Repository::discover(".").context("Not a git repository")
}

fn main_tree(repo: &Repository) -> Result<git2::Tree<'_>> {
    let branch = repo
        .find_branch("main", git2::BranchType::Local)
        .context("Could not find 'main' branch")?;
    let commit = branch.get().peel_to_commit()?;
    Ok(commit.tree()?)
}

fn diff_against_main(repo: &Repository, context_lines: u32) -> Result<Diff<'_>> {
    let tree = main_tree(repo)?;
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.show_untracked_content(true);
    opts.recurse_untracked_dirs(true);
    opts.context_lines(context_lines);
    repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
        .context("Failed to compute diff against main")
}

pub(crate) fn delta_to_change_type(delta: Delta) -> Option<ChangeType> {
    match delta {
        Delta::Added | Delta::Untracked => Some(ChangeType::Added),
        Delta::Deleted => Some(ChangeType::Deleted),
        Delta::Modified => Some(ChangeType::Modified),
        Delta::Renamed => Some(ChangeType::Renamed),
        _ => None,
    }
}

pub(crate) fn changed_files_from_diff(diff: &Diff<'_>) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    for (i, delta) in diff.deltas().enumerate() {
        let Some(change_type) = delta_to_change_type(delta.status()) else {
            continue;
        };
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();

        let (additions, deletions) = Patch::from_diff(diff, i)
            .ok()
            .flatten()
            .and_then(|p| p.line_stats().ok())
            .map(|(_ctx, add, del)| (add, del))
            .unwrap_or((0, 0));

        files.push(ChangedFile {
            path,
            change_type,
            additions,
            deletions,
        });
    }
    files
}

pub(crate) fn format_diff_patch(diff: &Diff<'_>) -> Result<String> {
    let mut output = String::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let origin = line.origin();
        // Only prepend origin for actual diff lines (+, -, space)
        // File headers (F/>) and hunk headers (H) are emitted as-is
        match origin {
            '+' | '-' | ' ' => output.push(origin),
            _ => {}
        }
        if let Ok(content) = std::str::from_utf8(line.content()) {
            output.push_str(content);
        }
        true
    })?;
    Ok(output)
}

#[cfg(not(tarpaulin_include))]
pub fn get_changed_files() -> Result<Vec<ChangedFile>> {
    let repo = open_repo()?;
    let diff = diff_against_main(&repo, 3)?;
    Ok(changed_files_from_diff(&diff))
}

#[cfg(not(tarpaulin_include))]
pub fn get_file_diff(path: &str) -> Result<String> {
    let repo = open_repo()?;
    let tree = main_tree(&repo)?;

    // Use full-file context: read line count from working copy
    let line_count = std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0) as u32;

    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    opts.include_untracked(true);
    opts.show_untracked_content(true);
    opts.recurse_untracked_dirs(true);
    opts.context_lines(line_count);

    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
        .context("Failed to compute file diff")?;

    format_diff_patch(&diff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Create a temp repo with an initial commit on "main" branch.
    fn setup_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Configure a dummy author for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();

        // Create initial commit on main
        let file_path = dir.path().join("initial.txt");
        fs::write(&file_path, "hello\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("initial.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let sig = repo.signature().unwrap();
        let commit_oid = {
            let tree = repo.find_tree(tree_oid).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
                .unwrap()
        };

        // Create "main" branch pointing to this commit
        {
            let commit = repo.find_commit(commit_oid).unwrap();
            repo.branch("main", &commit, false).unwrap();
        }

        (dir, repo)
    }

    // ── delta_to_change_type ─────────────────────────────────────────

    #[test]
    fn delta_to_change_type_added() {
        assert_eq!(delta_to_change_type(Delta::Added), Some(ChangeType::Added));
    }

    #[test]
    fn delta_to_change_type_untracked() {
        assert_eq!(
            delta_to_change_type(Delta::Untracked),
            Some(ChangeType::Added)
        );
    }

    #[test]
    fn delta_to_change_type_deleted() {
        assert_eq!(
            delta_to_change_type(Delta::Deleted),
            Some(ChangeType::Deleted)
        );
    }

    #[test]
    fn delta_to_change_type_modified() {
        assert_eq!(
            delta_to_change_type(Delta::Modified),
            Some(ChangeType::Modified)
        );
    }

    #[test]
    fn delta_to_change_type_renamed() {
        assert_eq!(
            delta_to_change_type(Delta::Renamed),
            Some(ChangeType::Renamed)
        );
    }

    #[test]
    fn delta_to_change_type_ignored_returns_none() {
        assert_eq!(delta_to_change_type(Delta::Ignored), None);
    }

    #[test]
    fn delta_to_change_type_unmodified_returns_none() {
        assert_eq!(delta_to_change_type(Delta::Unmodified), None);
    }

    // ── changed_files_from_diff ──────────────────────────────────────

    #[test]
    fn changed_files_detects_modified_file() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("initial.txt"), "hello\nworld\n").unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "initial.txt");
        assert_eq!(files[0].change_type, ChangeType::Modified);
        assert_eq!(files[0].additions, 1); // added "world"
    }

    #[test]
    fn changed_files_detects_untracked_file() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("new_file.txt"), "brand new\n").unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        let new = files.iter().find(|f| f.path == "new_file.txt").unwrap();
        assert_eq!(new.change_type, ChangeType::Added);
        assert_eq!(new.additions, 1);
    }

    #[test]
    fn changed_files_detects_deleted_file() {
        let (dir, repo) = setup_repo();
        fs::remove_file(dir.path().join("initial.txt")).unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "initial.txt");
        assert_eq!(files[0].change_type, ChangeType::Deleted);
        assert_eq!(files[0].deletions, 1);
    }

    #[test]
    fn changed_files_no_changes_returns_empty() {
        let (_dir, repo) = setup_repo();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        assert!(files.is_empty());
    }

    #[test]
    fn changed_files_multiple_changes() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("initial.txt"), "changed\n").unwrap();
        fs::write(dir.path().join("added.txt"), "new content\n").unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        assert_eq!(files.len(), 2);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"initial.txt"));
        assert!(paths.contains(&"added.txt"));
    }

    #[test]
    fn changed_files_untracked_in_subdirectory() {
        let (dir, repo) = setup_repo();
        let subdir = dir.path().join("src");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("lib.rs"), "fn main() {}\n").unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let files = changed_files_from_diff(&diff);

        let new = files.iter().find(|f| f.path == "src/lib.rs").unwrap();
        assert_eq!(new.change_type, ChangeType::Added);
    }

    // ── format_diff_patch ────────────────────────────────────────────

    #[test]
    fn format_diff_patch_modified_file() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("initial.txt"), "hello\nworld\n").unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let patch = format_diff_patch(&diff).unwrap();

        assert!(patch.contains("+world"));
        assert!(patch.contains("@@ "));
    }

    #[test]
    fn format_diff_patch_untracked_file() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("brand_new.txt"), "line1\nline2\n").unwrap();

        let mut opts = DiffOptions::new();
        opts.pathspec("brand_new.txt");
        opts.include_untracked(true);
        opts.show_untracked_content(true);

        let tree = main_tree(&repo).unwrap();
        let diff = repo
            .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
            .unwrap();
        let patch = format_diff_patch(&diff).unwrap();

        assert!(patch.contains("+line1"));
        assert!(patch.contains("+line2"));
    }

    #[test]
    fn format_diff_patch_deleted_file() {
        let (dir, repo) = setup_repo();
        fs::remove_file(dir.path().join("initial.txt")).unwrap();

        let diff = diff_against_main(&repo, 3).unwrap();
        let patch = format_diff_patch(&diff).unwrap();

        assert!(patch.contains("-hello"));
    }

    #[test]
    fn format_diff_patch_empty_diff() {
        let (_dir, repo) = setup_repo();

        let diff = diff_against_main(&repo, 3).unwrap();
        let patch = format_diff_patch(&diff).unwrap();

        assert!(patch.is_empty());
    }

    #[test]
    fn format_diff_patch_context_lines() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("initial.txt"), "hello\nnew line\n").unwrap();

        // Request full context
        let tree = main_tree(&repo).unwrap();
        let mut opts = DiffOptions::new();
        opts.pathspec("initial.txt");
        opts.include_untracked(true);
        opts.context_lines(100);

        let diff = repo
            .diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
            .unwrap();
        let patch = format_diff_patch(&diff).unwrap();

        // Context line for unchanged "hello" and addition of "new line"
        assert!(patch.contains(" hello"));
        assert!(patch.contains("+new line"));
    }
}
