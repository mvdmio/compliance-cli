//! The timing report: every command a Scenario runs, with its wall-clock time, in one JSON file per run under
//! the Cargo target folder's `e2e/`. It is rewritten after each command, so an aborted run keeps what ran. No
//! timing ever fails a run.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

struct Report {
    path: PathBuf,
    started_at_unix_milliseconds: u128,
    commands: Vec<Value>,
}

fn report() -> &'static Mutex<Report> {
    static REPORT: OnceLock<Mutex<Report>> = OnceLock::new();
    REPORT.get_or_init(|| {
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_millis();
        Mutex::new(Report {
            path: target_dir()
                .join("e2e")
                .join(format!("run-{started_at}.json")),
            started_at_unix_milliseconds: started_at,
            commands: Vec::new(),
        })
    })
}

/// The Cargo target folder: the nearest folder above the built binary that holds Cargo's `CACHEDIR.TAG`, so a
/// `--target <triple>` build still reports under `target/e2e/`.
fn target_dir() -> PathBuf {
    let binary = Path::new(env!("CARGO_BIN_EXE_compliance"));
    binary
        .ancestors()
        .find(|folder| folder.join("CACHEDIR.TAG").is_file())
        .or_else(|| binary.ancestors().nth(2))
        .expect("the binary sits under the target folder")
        .to_path_buf()
}

/// Adds one command to the report and rewrites the file.
pub fn record(scenario: &str, args: &[&str], exit_code: i32, elapsed: Duration) {
    let mut report = report()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    report.commands.push(json!({
        "scenario": scenario,
        "args": args,
        "exitCode": exit_code,
        "milliseconds": elapsed.as_millis(),
    }));
    let body = json!({
        "startedAtUnixMilliseconds": report.started_at_unix_milliseconds,
        "commands": report.commands,
    });
    // Ignored: a report that cannot be written must not fail the run.
    if let Some(folder) = report.path.parent() {
        let _ = fs::create_dir_all(folder);
    }
    let _ = fs::write(&report.path, format!("{body:#}\n"));
}
