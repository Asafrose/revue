use std::process::Command;

#[test]
fn test_git_diff_stat_parses() {
    let output = Command::new("git")
        .args(["diff", "--stat", "main...HEAD"])
        .output()
        .expect("git command failed");
    assert!(output.status.success());
}
