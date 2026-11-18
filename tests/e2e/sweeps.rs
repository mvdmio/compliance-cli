use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use crate::lookup::{Lookup, Verdict};
use crate::operations::{CommandName, live_operations};
use crate::test_bed::Scenario;
use crate::write_sweep;

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn help_sweep() {
    let scenario = Scenario::start("help sweep");
    let operations = live_operations(&scenario.test_bed().description());

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

struct Checked {
    label: String,
    verdict: Verdict,
}

struct ReadCommand {
    args: Vec<String>,
    out: Option<PathBuf>,
}

struct ReadSweep<'a> {
    lookup: Lookup<'a>,
    out: tempfile::TempDir,
}

impl ReadSweep<'_> {
    fn check(&mut self, index: usize) -> Checked {
        let operation = &self.lookup.operations[index];
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

    fn read_command(&mut self, index: usize) -> Result<ReadCommand, Verdict> {
        let operations = self.lookup.operations;
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
        args.extend(self.lookup.path_values(&operation.path)?);
        args.extend(self.lookup.query_args(&operation.path)?);
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

    fn judge(&mut self, read: ReadCommand) -> Verdict {
        let shown = format!("compliance {}", read.args.join(" "));
        let run = self.lookup.run(read.args);
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

    fn help_lists(&mut self, command: &CommandName) -> bool {
        let run = self
            .lookup
            .run(vec![command.group.clone(), "--help".to_string()]);
        run.code == 0
            && run
                .stdout
                .lines()
                .skip_while(|line| line.trim() != "Commands:")
                .skip(1)
                .take_while(|line| !line.trim().is_empty())
                .any(|line| line.split_whitespace().next() == Some(command.action.as_str()))
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
        lookup: Lookup::new(&scenario, &operations),
        out: tempfile::tempdir().expect("a temporary folder for the downloads"),
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

#[test]
#[ignore = "E2E: needs a Test-bed; run with `cargo test --test e2e -- --ignored`"]
fn write_sweep() {
    let scenario = Scenario::start("write sweep");
    let operations = live_operations(&scenario.test_bed().description());
    write_sweep::run(&scenario, &operations);
}
