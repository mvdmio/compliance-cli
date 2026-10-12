mod support;

use std::fs;
use std::path::Path;

use support::{FakeServer, Reply, mvdm};

#[test]
fn skill_prints_the_repo_skill_file_without_a_request() {
    let server = FakeServer::start(|_| Reply::empty(500));
    let repo_file = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("SKILL.md"))
        .expect("read SKILL.md");

    let run = mvdm(&["skill"], &[("MVDM_COMPLIANCE_URL", &server.url())]);

    assert_eq!(run.code, 0);
    assert_eq!(run.stdout, repo_file);
    assert!(run.stderr.is_empty());
    assert!(server.requests().is_empty());
}

#[test]
fn skill_starts_with_front_matter_a_skill_loader_accepts() {
    let run = mvdm(&["skill"], &[]);

    let front_matter = run
        .stdout
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map(|(front_matter, _)| front_matter)
        .expect("front matter between --- lines");
    assert!(front_matter.lines().any(|line| line == "name: mvdm"));
    assert!(
        front_matter
            .lines()
            .any(|line| line.starts_with("description: ") && line.len() > "description: ".len())
    );
}

#[test]
fn skill_names_no_product_and_points_at_the_product_help() {
    let run = mvdm(&["skill"], &[]);

    let text = run.stdout.to_lowercase();
    for product in ["compliance", "commonplace", "translations", "statistics"] {
        assert!(!text.contains(product), "the skill names {product}");
    }
    assert!(run.stdout.contains("mvdm --help"));
    assert!(run.stdout.contains("mvdm <product> --help"));
}
