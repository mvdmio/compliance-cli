mod support;

use support::compliance;

#[test]
fn version_prints_the_crate_version_as_plain_text() {
    let run = compliance(&["--version"], &[]);

    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout,
        format!("compliance {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_command_is_a_usage_mistake() {
    let run = compliance(&[], &[]);

    assert_eq!(run.code, 2);
    assert_eq!(run.stderr_json()["error"], "usage");
    assert!(run.stdout.is_empty());
}
