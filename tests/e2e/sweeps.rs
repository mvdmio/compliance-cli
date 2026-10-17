//! The Sweeps: Scenarios that run one command per operation in the Test-bed's live API description. Each runs
//! every command before it fails, then fails once, listing every command that did not pass.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::operations::{CommandName, LiveOperation, live_operations, placeholders};
use crate::support::Run;
use crate::test_bed::Scenario;

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn help_sweep() {
    let scenario = Scenario::start("help sweep");
    let operations = live_operations(&scenario.test_bed().description());

    // Every generated command, once: two operations with one name are one command, and the CLI keeps the first.
    let mut commands: Vec<&CommandName> = Vec::new();
    for command in operations
        .iter()
        .filter_map(|operation| operation.command.as_ref())
    {
        if command.is_generated() && !commands.contains(&command) {
            commands.push(command);
        }
    }
    assert!(
        !commands.is_empty(),
        "the live description yields no generated command"
    );

    let mut failures = Vec::new();
    for command in &commands {
        let run = scenario.run(&[&command.group, &command.action, "--help"]);
        if run.code != 0 || !run.stdout.contains("Usage:") {
            failures.push(format!(
                "{command} --help: exit {}, stdout {:?}, stderr {:?}",
                run.code, run.stdout, run.stderr
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} commands did not answer --help:\n{}",
        failures.len(),
        commands.len(),
        failures.join("\n")
    );
}

/// Where a value comes from: the field `field` of the first item in the list at `list`, a GET path in the live
/// description whose placeholders the read's own path fills.
struct Source {
    list: &'static str,
    field: &'static str,
}

/// A path parameter the path rule cannot explain.
struct PathSource {
    /// The read's path up to and including the parameter.
    read: &'static str,
    source: Source,
}

/// A query parameter a read needs although the description does not mark it required.
struct QuerySource {
    /// The read's path.
    read: &'static str,
    /// The option the CLI names the parameter by.
    option: &'static str,
    source: Source,
}

/// Every path parameter not listed here takes the `id` of the first item in the list whose path the read extends:
/// `/api/v1/risks/{id}/comments` takes it from `/api/v1/risks`.
const PATH_SOURCES: [PathSource; 3] = [
    // A Check Run has no list of its own: every Check Result names the run it came from.
    PathSource {
        read: "/api/v1/check-runs/{id}",
        source: Source {
            list: "/api/v1/check-results",
            field: "checkRunId",
        },
    },
    // A Requirement is addressed by its code, not its id.
    PathSource {
        read: "/api/v1/frameworks/{frameworkId}/requirements/{code}",
        source: Source {
            list: "/api/v1/frameworks/{frameworkId}/requirements",
            field: "code",
        },
    },
    // Conversation messages are read by the Conversation's id, not a message's.
    PathSource {
        read: "/api/v1/conversations/{id}",
        source: Source {
            list: "/api/v1/conversations",
            field: "id",
        },
    },
];

const QUERY_SOURCES: [QuerySource; 1] = [
    // `invitations.preview` answers 422 without at least one Person.
    QuerySource {
        read: "/api/v1/invitations/preview",
        option: "--person-ids",
        source: Source {
            list: "/api/v1/people",
            field: "id",
        },
    },
];

/// How one read went. A skipped read depends on an empty list.
enum Verdict {
    Passed,
    Skipped(String),
    Failed(String),
}

/// One read in the report: what it is, and how it went.
struct Checked {
    label: String,
    verdict: Verdict,
}

/// The command line for one read, and the file it downloads to when its answer is a file.
struct ReadCommand {
    args: Vec<String>,
    out: Option<PathBuf>,
}

/// The Read sweep's state: every command it ran, so a list that finds ids runs once, as its own read too.
struct ReadSweep<'a> {
    scenario: &'a Scenario,
    operations: &'a [LiveOperation],
    /// Where the downloads write their files.
    out: tempfile::TempDir,
    runs: HashMap<Vec<String>, Run>,
}

impl ReadSweep<'_> {
    /// Runs `compliance <args>` once; a second call with the same arguments answers with the first run.
    fn run(&mut self, args: Vec<String>) -> &Run {
        let scenario = self.scenario;
        self.runs.entry(args).or_insert_with_key(|args| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            scenario.run(&args)
        })
    }

    /// Runs the read at `operations[index]` through its command. `accounts.list` needs no special case: its name
    /// is the hand-written `accounts list`, which answers in place of the generated one.
    fn check(&mut self, index: usize) -> Checked {
        let operation = &self.operations[index];
        let label = match &operation.command {
            Some(command) => format!("GET {} as `{command}`", operation.path),
            None => format!("GET {}", operation.path),
        };
        let verdict = match self.read_command(index) {
            Ok(read) => self.judge(read),
            Err(verdict) => verdict,
        };
        Checked { label, verdict }
    }

    /// The command line for the read at `operations[index]`, or why it cannot run: the CLI dropped the operation
    /// (no name, a name an earlier operation holds, or a name `--help` does not list), or a list it takes a value
    /// from is empty or failed.
    fn read_command(&mut self, index: usize) -> Result<ReadCommand, Verdict> {
        let operations = self.operations;
        let operation = &operations[index];
        let Some(command) = &operation.command else {
            return Err(Verdict::Failed(
                "no `<group>.<action>` operationId, so the CLI offers no command".into(),
            ));
        };
        if let Some(earlier) = operations[..index]
            .iter()
            .find(|earlier| earlier.command.as_ref() == Some(command))
        {
            return Err(Verdict::Failed(format!(
                "the CLI runs the earlier {} {} under this name",
                earlier.method, earlier.path
            )));
        }
        if !self.help_lists(command) {
            return Err(Verdict::Failed(format!(
                "`compliance {} --help` does not list `{}`",
                command.group, command.action
            )));
        }

        let mut args = command.args();
        args.extend(self.path_values(&operation.path)?);
        if let Some(query) = QUERY_SOURCES
            .iter()
            .find(|query| query.read == operation.path)
        {
            let value = self.first_value(query.source.list, query.source.field, &[])?;
            args.extend([query.option.to_string(), value]);
        }
        let out = operation.download.then(|| {
            self.out
                .path()
                .join(format!("{}-{}", command.group, command.action))
        });
        if let Some(out) = &out {
            args.extend(["--out".to_string(), out.display().to_string()]);
        }
        Ok(ReadCommand { args, out })
    }

    /// Runs one read. It passes on exit 0 and JSON on stdout, or, for a download, a non-empty file at `--out`.
    fn judge(&mut self, read: ReadCommand) -> Verdict {
        let shown = format!("compliance {}", read.args.join(" "));
        let run = self.run(read.args);
        if run.code != 0 {
            return Verdict::Failed(format!(
                "`{shown}` exited {}: {}",
                run.code,
                run.stderr.trim()
            ));
        }
        let problem = match read.out {
            Some(out) => match fs::metadata(&out) {
                Ok(file) if file.len() > 0 => return Verdict::Passed,
                Ok(_) => "wrote an empty file".to_string(),
                Err(error) => format!("wrote no file: {error}"),
            },
            None if serde_json::from_str::<Value>(&run.stdout).is_ok() => return Verdict::Passed,
            None => format!("printed no JSON: {:?}", run.stdout),
        };
        Verdict::Failed(format!("`{shown}` {problem}"))
    }

    /// Whether `compliance <group> --help` lists the action under `Commands:`.
    fn help_lists(&mut self, command: &CommandName) -> bool {
        let run = self.run(vec![command.group.clone(), "--help".to_string()]);
        run.code == 0
            && run
                .stdout
                .lines()
                .skip_while(|line| line.trim() != "Commands:")
                .skip(1)
                .take_while(|line| !line.trim().is_empty())
                .any(|line| line.split_whitespace().next() == Some(command.action.as_str()))
    }

    /// A value for every placeholder in `path`, in order, each from the first item of the list it depends on.
    fn path_values(&mut self, path: &str) -> Result<Vec<String>, Verdict> {
        let mut known: Vec<(&str, String)> = Vec::new();
        for name in placeholders(path) {
            let placeholder = format!("{{{name}}}");
            let Some((before, _)) = path.split_once(&placeholder) else {
                continue;
            };
            let read = format!("{before}{placeholder}");
            let (list, field) = match PATH_SOURCES.iter().find(|path| path.read == read) {
                Some(path) => (path.source.list, path.source.field),
                None => (before.trim_end_matches('/'), "id"),
            };
            let value = self.first_value(list, field, &known)?;
            known.push((name, value));
        }
        Ok(known.into_iter().map(|(_, value)| value).collect())
    }

    /// `field` of the first item that the list at `list` prints, its placeholders filled from `known`. An empty
    /// list skips the read that depends on it.
    fn first_value(
        &mut self,
        list: &str,
        field: &str,
        known: &[(&str, String)],
    ) -> Result<String, Verdict> {
        let operations = self.operations;
        let Some(command) = operations
            .iter()
            .find(|operation| operation.is_read() && operation.path == list)
            .and_then(|operation| operation.command.as_ref())
        else {
            return Err(Verdict::Failed(format!(
                "no read at `{list}` to take `{field}` from"
            )));
        };
        let mut args = command.args();
        for name in placeholders(list) {
            let Some((_, value)) = known.iter().find(|(known, _)| *known == name) else {
                return Err(Verdict::Failed(format!(
                    "no value for `{{{name}}}` in `{list}`"
                )));
            };
            args.push(value.clone());
        }

        let shown = format!("compliance {}", args.join(" "));
        let run = self.run(args);
        let printed = match serde_json::from_str::<Value>(&run.stdout) {
            Ok(printed) if run.code == 0 => printed,
            _ => {
                return Err(Verdict::Failed(format!(
                    "the list `{shown}` it takes `{field}` from failed with exit {}",
                    run.code
                )));
            }
        };
        // A page (`{"items": [...]}`), or a bare array.
        let Some(items) = printed["items"].as_array().or(printed.as_array()) else {
            return Err(Verdict::Failed(format!(
                "the list `{shown}` prints no `items`"
            )));
        };
        let Some(first) = items.first() else {
            return Err(Verdict::Skipped(format!("the list `{shown}` is empty")));
        };
        match &first[field] {
            Value::String(text) if !text.is_empty() => Ok(text.clone()),
            Value::Number(number) => Ok(number.to_string()),
            _ => Err(Verdict::Failed(format!(
                "the first item of `{shown}` has no `{field}`"
            ))),
        }
    }
}

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn read_sweep() {
    let scenario = Scenario::start("read sweep");
    let operations = live_operations(&scenario.test_bed().description());
    let reads: Vec<usize> = (0..operations.len())
        .filter(|&index| operations[index].is_read())
        .collect();
    assert!(
        !reads.is_empty(),
        "the live description has no GET operation"
    );

    let mut sweep = ReadSweep {
        scenario: &scenario,
        operations: &operations,
        out: tempfile::tempdir().expect("a temporary folder for the downloads"),
        runs: HashMap::new(),
    };
    let (mut passed, mut skipped, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for &index in &reads {
        let Checked { label, verdict } = sweep.check(index);
        match verdict {
            Verdict::Passed => passed.push(label),
            Verdict::Skipped(reason) => skipped.push(format!("{label}: {reason}")),
            Verdict::Failed(reason) => failed.push(format!("{label}: {reason}")),
        }
    }

    println!(
        "Read sweep: {} reads, {} passed, {} skipped, {} failed",
        reads.len(),
        passed.len(),
        skipped.len(),
        failed.len()
    );
    for label in &passed {
        println!("passed: {label}");
    }
    for line in &skipped {
        println!("skipped: {line}");
    }
    for line in &failed {
        println!("failed: {line}");
    }
    assert!(
        failed.is_empty(),
        "{} of {} reads failed:\n{}",
        failed.len(),
        reads.len(),
        failed.join("\n")
    );
}
