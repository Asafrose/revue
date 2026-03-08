use git2::Repository;

#[test]
fn repo_discovers_from_cwd() {
    let repo = Repository::discover(".");
    assert!(repo.is_ok(), "Should find a git repository from cwd");
}
