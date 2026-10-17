mod support;

use std::fs;
use std::path::Path;

use support::{FakeServer, Reply, compliance};

#[test]
fn skill_prints_the_repo_skill_file_without_a_request() {
    let server = FakeServer::start(|_| Reply::empty(500));
    let repo_file = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("SKILL.md"))
        .expect("read SKILL.md");

    let run = compliance(&["skill"], &[("COMPLIANCE_URL", &server.url())]);

    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, repo_file);
    assert!(run.stderr.is_empty());
    assert!(server.requests().is_empty());
}

#[test]
fn skill_starts_with_front_matter_a_skill_loader_accepts() {
    let run = compliance(&["skill"], &[]);

    let front_matter = run
        .stdout
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front_matter, _)| front_matter)
        .expect("front matter between --- lines");
    assert!(front_matter.lines().any(|line| line == "name: compliance"));
    assert!(
        front_matter
            .lines()
            .any(|line| line.starts_with("description: ") && line.len() > "description: ".len())
    );
}
