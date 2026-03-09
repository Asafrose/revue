use anyhow::{Context, Result};
use git2::{Delta, Diff, DiffOptions, Oid, Patch, Repository};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ChangedFile {
    pub path: String,
    pub change_type: ChangeType,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: Oid,
    pub short_id: String,
    pub message: String,
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

fn main_commit(repo: &Repository) -> Result<git2::Commit<'_>> {
    let branch = repo
        .find_branch("main", git2::BranchType::Local)
        .context("Could not find 'main' branch")?;
    Ok(branch.get().peel_to_commit()?)
}

fn main_tree(repo: &Repository) -> Result<git2::Tree<'_>> {
    Ok(main_commit(repo)?.tree()?)
}

/// List commits from HEAD back to (but not including) the main branch commit.
/// Returns newest commit first.
pub(crate) fn list_commits(repo: &Repository) -> Result<Vec<CommitInfo>> {
    let main = main_commit(repo)?;
    let head = repo.head()?.peel_to_commit()?;

    let mut revwalk = repo.revwalk()?;
    revwalk.push(head.id())?;
    revwalk.hide(main.id())?;
    revwalk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let short_id = format!("{}", &oid.to_string()[..7]);
        let message = commit
            .message()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        commits.push(CommitInfo {
            id: oid,
            short_id,
            message,
        });
    }
    Ok(commits)
}

/// Compute diff between two tree-ish objects (commits).
/// If `to_oid` is None, diffs to the working directory.
pub(crate) fn diff_commit_range(
    repo: &Repository,
    from_oid: Oid,
    to_oid: Option<Oid>,
    context_lines: u32,
) -> Result<Diff<'_>> {
    let from_commit = repo.find_commit(from_oid)?;
    let from_tree = from_commit.tree()?;
    let mut opts = DiffOptions::new();
    opts.context_lines(context_lines);
    opts.include_untracked(true);
    opts.show_untracked_content(true);
    opts.recurse_untracked_dirs(true);

    match to_oid {
        Some(to) => {
            let to_commit = repo.find_commit(to)?;
            let to_tree = to_commit.tree()?;
            repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))
                .context("Failed to diff commit range")
        }
        None => repo
            .diff_tree_to_workdir_with_index(Some(&from_tree), Some(&mut opts))
            .context("Failed to diff to working directory"),
    }
}

/// Compute diff for a specific file within a commit range.
pub(crate) fn diff_commit_range_file<'a>(
    repo: &'a Repository,
    from_oid: Oid,
    to_oid: Option<Oid>,
    path: &str,
    context_lines: u32,
) -> Result<Diff<'a>> {
    let from_commit = repo.find_commit(from_oid)?;
    let from_tree = from_commit.tree()?;
    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    opts.context_lines(context_lines);
    opts.include_untracked(true);
    opts.show_untracked_content(true);
    opts.recurse_untracked_dirs(true);

    match to_oid {
        Some(to) => {
            let to_commit = repo.find_commit(to)?;
            let to_tree = to_commit.tree()?;
            repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))
                .context("Failed to diff file in commit range")
        }
        None => repo
            .diff_tree_to_workdir_with_index(Some(&from_tree), Some(&mut opts))
            .context("Failed to diff file to working directory"),
    }
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
pub fn get_changed_files_for_range(from: Oid, to: Option<Oid>) -> Result<Vec<ChangedFile>> {
    let repo = open_repo()?;
    let diff = diff_commit_range(&repo, from, to, 3)?;
    Ok(changed_files_from_diff(&diff))
}

#[cfg(not(tarpaulin_include))]
pub fn get_commits() -> Result<Vec<CommitInfo>> {
    let repo = open_repo()?;
    list_commits(&repo)
}

#[cfg(not(tarpaulin_include))]
pub fn get_main_oid() -> Result<Oid> {
    let repo = open_repo()?;
    let oid = main_commit(&repo)?.id();
    Ok(oid)
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

#[cfg(not(tarpaulin_include))]
pub fn get_file_diff_for_range(path: &str, from: Oid, to: Option<Oid>) -> Result<String> {
    let repo = open_repo()?;

    let line_count = std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0) as u32;

    let diff = diff_commit_range_file(&repo, from, to, path, line_count)?;
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

    // ── commit helpers ──────────────────────────────────────────────

    /// Create a commit on HEAD in the given repo.
    fn make_commit(dir: &std::path::Path, repo: &Repository, filename: &str, content: &str) {
        fs::write(dir.join(filename), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(filename)).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = repo.signature().unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("add {}", filename),
            &tree,
            &[&parent],
        )
        .unwrap();
    }

    // ── list_commits ────────────────────────────────────────────────

    #[test]
    fn list_commits_no_commits_beyond_main() {
        let (_dir, repo) = setup_repo();
        let commits = list_commits(&repo).unwrap();
        assert!(commits.is_empty());
    }

    #[test]
    fn list_commits_returns_commits_after_main() {
        let (dir, repo) = setup_repo();
        make_commit(dir.path(), &repo, "a.txt", "aaa\n");
        make_commit(dir.path(), &repo, "b.txt", "bbb\n");

        let commits = list_commits(&repo).unwrap();
        assert_eq!(commits.len(), 2);
        // Newest first
        assert!(commits[0].message.contains("add b.txt"));
        assert!(commits[1].message.contains("add a.txt"));
        assert_eq!(commits[0].short_id.len(), 7);
    }

    // ── diff_commit_range ───────────────────────────────────────────

    #[test]
    fn diff_commit_range_between_two_commits() {
        let (dir, repo) = setup_repo();
        make_commit(dir.path(), &repo, "a.txt", "aaa\n");
        let commits = list_commits(&repo).unwrap();
        let main_oid = main_commit(&repo).unwrap().id();

        let diff = diff_commit_range(&repo, main_oid, Some(commits[0].id), 3).unwrap();
        let files = changed_files_from_diff(&diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.txt");
    }

    #[test]
    fn diff_commit_range_to_working_dir() {
        let (dir, repo) = setup_repo();
        fs::write(dir.path().join("initial.txt"), "changed\n").unwrap();
        let main_oid = main_commit(&repo).unwrap().id();

        let diff = diff_commit_range(&repo, main_oid, None, 3).unwrap();
        let files = changed_files_from_diff(&diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].change_type, ChangeType::Modified);
    }
}
