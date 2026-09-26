//! The operations in an API description, and the commands the CLI names them by.
//!
//! The crate has no library target, so the rules below are copies of the CLI's own: `operations`, `is_download`,
//! `placeholders`, and `kebab_case` in `src/openapi.rs`, `is_json` in `src/response.rs`, and `tree` in
//! `src/generated.rs`. `tests/generated.rs` checks them against the binary on every push, so a change there that
//! these do not follow fails before the nightly E2E run.

// Shared with `tests/generated.rs`, which uses only part of it.
#![allow(dead_code)]

use serde_json::Value;
use ureq::http::Method;

/// The methods the CLI reads operations under (`METHODS` in `src/openapi.rs`).
const METHODS: [(&str, Method); 7] = [
    ("get", Method::GET),
    ("put", Method::PUT),
    ("post", Method::POST),
    ("delete", Method::DELETE),
    ("options", Method::OPTIONS),
    ("head", Method::HEAD),
    ("patch", Method::PATCH),
];

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

    /// `<group> <action>`, the start of the command's arguments.
    pub fn args(&self) -> Vec<String> {
        vec![self.group.clone(), self.action.clone()]
    }
}

impl std::fmt::Display for CommandName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "compliance {} {}", self.group, self.action)
    }
}

/// One operation in the description, with the command the CLI names it by.
pub struct LiveOperation {
    /// `None` when the operation has no `<group>.<action>` `operationId`, so the CLI offers no command for it.
    pub command: Option<CommandName>,
    pub method: Method,
    /// The path template, such as `/api/v1/risks/{id}`.
    pub path: String,
    /// The answer is a file, not JSON, so the command needs `--out`.
    pub download: bool,
}

impl LiveOperation {
    pub fn is_read(&self) -> bool {
        self.method == Method::GET
    }
}

/// Every operation in `description`, in document order, as `operations` in `src/openapi.rs` reads them. Unlike
/// the CLI, it keeps an operation whose name an earlier one holds.
pub fn live_operations(description: &Value) -> Vec<LiveOperation> {
    let Some(paths) = description["paths"].as_object() else {
        panic!("the description has no `paths`");
    };
    let mut operations = Vec::new();
    for (path, item) in paths {
        let item = resolve(description, item);
        for (key, method) in &METHODS {
            let Some(operation) = item.get(*key) else {
                continue;
            };
            operations.push(LiveOperation {
                command: operation["operationId"]
                    .as_str()
                    .and_then(CommandName::from_operation_id),
                method: method.clone(),
                path: path.clone(),
                download: is_download(description, &operation["responses"]),
            });
        }
    }
    operations
}

/// The placeholder names in a path template, in order (`placeholders` in `src/openapi.rs`).
pub fn placeholders(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
        .collect()
}

/// The first 2xx response has content, and none of it is JSON (`is_download` in `src/openapi.rs`).
fn is_download(document: &Value, responses: &Value) -> bool {
    let Some(responses) = responses.as_object() else {
        return false;
    };
    let mut success: Vec<(&String, &Value)> = responses
        .iter()
        .filter(|(status, _)| status.starts_with('2'))
        .collect();
    success.sort_by_key(|(status, _)| status.as_str());
    let Some(content) = success
        .first()
        .and_then(|(_, response)| resolve(document, response)["content"].as_object())
    else {
        return false;
    };
    !content.is_empty() && !content.keys().any(|media_type| is_json(media_type))
}

/// A JSON media type (`is_json` in `src/response.rs`).
fn is_json(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
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
