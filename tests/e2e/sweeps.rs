//! The Sweeps: Scenarios that run one command per operation in the Test-bed's live API description. Each runs
//! every command before it fails, then fails once, listing every command that did not pass.
//!
//! The crate has no library target, so the rules below that name operations and commands are copies of the CLI's
//! own: `operations` and `kebab_case` in `src/openapi.rs`, and `tree` in `src/generated.rs`. They must follow any
//! change there.

use serde_json::Value;

use crate::test_bed::Scenario;

/// The methods the CLI reads operations under (`METHODS` in `src/openapi.rs`).
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

/// The hand-written commands that take no action (`Command` in `src/cli.rs`). A generated group of the same name is
/// hidden behind them, as `tree` in `src/generated.rs` does.
const HANDWRITTEN_LEAVES: [&str; 5] = ["login", "logout", "status", "api", "skill"];

/// The most `$ref` hops followed to reach a path item (`MAX_REF_DEPTH` in `src/openapi.rs`).
const MAX_REF_DEPTH: usize = 16;

/// A command the CLI names an operation by: `compliance <group> <action>`.
#[derive(Clone, PartialEq, Eq)]
pub struct CommandName {
    pub group: String,
    pub action: String,
}

impl CommandName {
    /// The CLI's rule: the `operationId` is split on its first `.` into group and action, and each part is
    /// kebab-cased. `None` when either part comes out empty.
    pub fn from_operation_id(operation_id: &str) -> Option<Self> {
        let (group, action) = operation_id.split_once('.')?;
        let (group, action) = (kebab_case(group), kebab_case(action));
        (!group.is_empty() && !action.is_empty()).then_some(CommandName { group, action })
    }

    /// Whether the CLI builds a generated command under this name: `tree` drops a group or action named `help`,
    /// and every group a hand-written command without actions hides. `accounts list` and `accounts switch` stay:
    /// the hand-written actions answer in their place.
    pub fn is_generated(&self) -> bool {
        self.group != "help"
            && self.action != "help"
            && !HANDWRITTEN_LEAVES.contains(&self.group.as_str())
    }
}

impl std::fmt::Display for CommandName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "compliance {} {}", self.group, self.action)
    }
}

/// One operation in the live description, with the command the CLI names it by.
pub struct LiveOperation {
    /// `None` when the operation has no `<group>.<action>` `operationId`, so the CLI offers no command for it.
    pub command: Option<CommandName>,
}

/// Every operation in `description`, in document order, as `operations` in `src/openapi.rs` reads them.
pub fn live_operations(description: &Value) -> Vec<LiveOperation> {
    let Some(paths) = description["paths"].as_object() else {
        panic!("the live description has no `paths`");
    };
    let mut operations = Vec::new();
    for item in paths.values() {
        let item = resolve(description, item);
        for method in METHODS {
            let Some(operation) = item.get(method) else {
                continue;
            };
            operations.push(LiveOperation {
                command: operation["operationId"]
                    .as_str()
                    .and_then(CommandName::from_operation_id),
            });
        }
    }
    operations
}

/// Follows a local `$ref` to what it names, as `resolve` in `src/openapi.rs` does.
fn resolve<'a>(document: &'a Value, value: &'a Value) -> &'a Value {
    let mut value = value;
    for _ in 0..MAX_REF_DEPTH {
        let Some(target) = value["$ref"]
            .as_str()
            .and_then(|reference| reference.strip_prefix('#'))
            .and_then(|pointer| document.pointer(pointer))
        else {
            break;
        };
        value = target;
    }
    value
}

/// Kebab case as the CLI writes it (`kebab_case` in `src/openapi.rs`): a word starts at an upper-case letter after a
/// lower-case letter or digit, or at the last capital of a run of capitals before a lower-case letter, and every
/// run of other characters becomes one `-`.
fn kebab_case(name: &str) -> String {
    let characters: Vec<char> = name.chars().collect();
    let mut kebab = String::new();
    for (index, &character) in characters.iter().enumerate() {
        if !character.is_ascii_alphanumeric() {
            if !kebab.is_empty() && !kebab.ends_with('-') {
                kebab.push('-');
            }
            continue;
        }
        if character.is_ascii_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let next = characters.get(index + 1).copied();
            let starts_word = previous.is_ascii_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_ascii_uppercase() && next.is_some_and(|c| c.is_ascii_lowercase()));
            if starts_word && !kebab.ends_with('-') {
                kebab.push('-');
            }
        }
        kebab.push(character.to_ascii_lowercase());
    }
    kebab.trim_end_matches('-').to_string()
}

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
