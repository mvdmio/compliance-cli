//! The Test-bed the E2E suite runs against, and the Scenario that runs the binary on it.
//!
//! The Test-bed comes from `COMPLIANCE_E2E_URL` and `COMPLIANCE_E2E_TOKEN`. When both are unset, the suite runs the
//! Launcher once per test process with `--until-stdin-closes` and holds its stdin open, so the Launcher stops and
//! cleans up when the test process ends, however it ends.

use std::env;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde_json::Value;
use tempfile::TempDir;

use crate::support::{Run, command};
use crate::timing;

/// A running Test-bed: its address and the Personal token of seed user 1.
pub struct TestBed {
    pub url: String,
    pub token: String,
}

impl TestBed {
    /// The Test-bed's live API description, fetched without a credential, as the CLI fetches it.
    pub fn description(&self) -> Value {
        let url = format!("{}/openapi/v1.json", self.url.trim_end_matches('/'));
        let mut response = ureq::get(&url)
            .call()
            .unwrap_or_else(|error| panic!("could not fetch {url}: {error}"));
        let text = response
            .body_mut()
            .with_config()
            .limit(u64::MAX)
            .read_to_string()
            .unwrap_or_else(|error| panic!("could not read {url}: {error}"));
        serde_json::from_str(&text).unwrap_or_else(|error| panic!("{url} is not JSON: {error}"))
    }
}

/// The Launcher this process started. It is never dropped: its stdin closes when the process ends.
struct Launcher {
    _child: Child,
    _stdin: ChildStdin,
    _stdout: BufReader<ChildStdout>,
}

struct Shared {
    test_bed: TestBed,
    _launcher: Option<Launcher>,
}

/// The one Test-bed every Scenario in this process shares.
fn test_bed() -> &'static TestBed {
    static SHARED: OnceLock<Result<Shared, String>> = OnceLock::new();
    match SHARED.get_or_init(find_or_start) {
        Ok(shared) => &shared.test_bed,
        Err(message) => panic!("{message}"),
    }
}

fn find_or_start() -> Result<Shared, String> {
    let url = env::var("COMPLIANCE_E2E_URL").ok();
    let token = env::var("COMPLIANCE_E2E_TOKEN").ok();
    match (url, token) {
        (Some(url), Some(token)) => Ok(Shared {
            test_bed: TestBed { url, token },
            _launcher: None,
        }),
        (None, None) => start_launcher(),
        _ => Err(
            "set both COMPLIANCE_E2E_URL and COMPLIANCE_E2E_TOKEN, or neither to start a Test-bed"
                .into(),
        ),
    }
}

fn start_launcher() -> Result<Shared, String> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/test-bed.sh");
    let mut child = Command::new("bash")
        .arg(&script)
        .arg("--until-stdin-closes")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("could not run the Launcher {}: {error}", script.display()))?;
    let stdin = child.stdin.take().expect("the Launcher's stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("the Launcher's stdout"));

    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .map_err(|error| format!("could not read the Launcher's output: {error}"))?;
    if line.is_empty() {
        let status = child.wait().map_err(|error| error.to_string())?;
        return Err(format!(
            "the Launcher stopped before it printed the Test-bed ({status}); its stderr says why"
        ));
    }
    let printed: Value = serde_json::from_str(&line)
        .map_err(|error| format!("the Launcher printed no JSON ({error}): {line}"))?;
    let field = |name: &str| {
        printed[name]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("the Launcher printed no `{name}`: {line}"))
    };
    Ok(Shared {
        test_bed: TestBed {
            url: field("url")?,
            token: field("token")?,
        },
        _launcher: Some(Launcher {
            _child: child,
            _stdin: stdin,
            _stdout: stdout,
        }),
    })
}

/// One Scenario: it holds the process-wide lock, so Scenarios run one at a time, and one config and cache folder,
/// so the API description is fetched once per Scenario.
pub struct Scenario {
    name: &'static str,
    test_bed: &'static TestBed,
    config: TempDir,
    _turn: MutexGuard<'static, ()>,
}

impl Scenario {
    pub fn start(name: &'static str) -> Self {
        static TURN: Mutex<()> = Mutex::new(());
        // A Scenario that failed poisons the lock; the next one still runs.
        let turn = TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        Scenario {
            name,
            test_bed: test_bed(),
            config: tempfile::tempdir().expect("a temporary config folder"),
            _turn: turn,
        }
    }

    pub fn test_bed(&self) -> &TestBed {
        self.test_bed
    }

    /// Runs `compliance <args>` against the Test-bed with its Personal token, and records the time it took.
    pub fn run(&self, args: &[&str]) -> Run {
        self.run_with_token(args, &self.test_bed.token)
    }

    /// Runs `compliance <args>` against the Test-bed with `token` in place of its Personal token.
    pub fn run_with_token(&self, args: &[&str], token: &str) -> Run {
        let env = [
            ("COMPLIANCE_URL", self.test_bed.url.as_str()),
            ("COMPLIANCE_TOKEN", token),
        ];
        // The config folder is also the working folder, so a file a command writes stays inside the Scenario.
        let folder = self.config.path();
        let mut command = command(folder, args, &env, folder);
        let started = Instant::now();
        let output = command.output().expect("run the compliance binary");
        let elapsed = started.elapsed();
        let run = Run::from_output(output);
        timing::record(self.name, args, run.code, elapsed);
        run
    }
}
