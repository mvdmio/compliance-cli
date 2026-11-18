//! The operations in an API description, and the commands the CLI names them by.
//!
//! The crate has no library target, so the rules below are copies of the CLI's own: `operations`, `is_download`,
//! `placeholders`, `parameters`, `body`, `body_schema`, `schema`, and `kebab_case` in `src/openapi.rs`, `is_json`
//! in `src/response.rs`, and `tree` and `claim` in `src/generated.rs`. `tests/generated.rs` checks them against the
//! binary on every push, so a change there that these do not follow fails before the nightly E2E run.

// Shared with `tests/generated.rs`, which uses only part of it.
#![allow(dead_code)]

use std::collections::HashSet;

use serde_json::Value;
use ureq::http::Method;

/// Option names no parameter may take (`GLOBAL_OPTIONS` in `src/generated.rs`).
pub const GLOBAL_OPTIONS: [&str; 4] = ["help", "body", "out", "file"];

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

/// The type a command option parses (`Kind` in `src/openapi.rs`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Integer,
    Number,
    Boolean,
    /// An object, a `oneOf`, or a `$ref`: given as JSON text.
    Json,
}

/// A query parameter or a top-level body field, with the option the CLI gives it.
pub struct LiveParameter {
    pub name: String,
    /// The `--<option>` the CLI names it by, after the clash rule.
    pub option: String,
    pub kind: Kind,
    /// Many values of `kind`, one per use of the option.
    pub array: bool,
    pub format: Option<String>,
    /// The allowed values of a string, integer, or number, as typed.
    pub choices: Vec<String>,
    pub required: bool,
    /// `x-upload-target`: the value is an upload id, which `--file` fills.
    pub upload_target: Option<String>,
}

/// One operation in the description, with the command the CLI names it by.
pub struct LiveOperation {
    /// The `operationId`, such as `risks.create`.
    pub id: String,
    /// `None` when the operation has no `<group>.<action>` `operationId`, so the CLI offers no command for it.
    pub command: Option<CommandName>,
    pub method: Method,
    /// The path template, such as `/api/v1/risks/{id}`.
    pub path: String,
    /// The answer is a file, not JSON, so the command needs `--out`.
    pub download: bool,
    pub query: Vec<LiveParameter>,
    /// The top-level properties of an object body: none without a body or with a `oneOf` body.
    pub body_fields: Vec<LiveParameter>,
}

impl LiveOperation {
    pub fn is_read(&self) -> bool {
        self.method == Method::GET
    }

    pub fn is_write(&self) -> bool {
        [Method::POST, Method::PUT, Method::PATCH, Method::DELETE].contains(&self.method)
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
            let (query, body_fields) = options(description, item, operation);
            operations.push(LiveOperation {
                id: operation["operationId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                command: operation["operationId"]
                    .as_str()
                    .and_then(CommandName::from_operation_id),
                method: method.clone(),
                path: path.clone(),
                download: is_download(description, &operation["responses"]),
                query,
                body_fields,
            });
        }
    }
    operations
}

/// The query parameters and body fields of `operation` with the option the CLI names each by. Names are claimed in
/// the CLI's order: the global options, the query parameters, the body fields.
fn options(
    document: &Value,
    item: &Value,
    operation: &Value,
) -> (Vec<LiveParameter>, Vec<LiveParameter>) {
    let mut taken: HashSet<String> = GLOBAL_OPTIONS.iter().map(|name| name.to_string()).collect();
    // The operation's own parameter replaces the path item's one of the same name and location.
    let mut listed: Vec<(&str, &str, &Value)> = Vec::new();
    for parameter in item["parameters"]
        .as_array()
        .into_iter()
        .chain(operation["parameters"].as_array())
        .flatten()
    {
        let parameter = resolve(document, parameter);
        let (Some(name), Some(location)) = (parameter["name"].as_str(), parameter["in"].as_str())
        else {
            continue;
        };
        listed.retain(|(known_location, known, _)| (*known_location, *known) != (location, name));
        listed.push((location, name, parameter));
    }
    let query = listed
        .iter()
        .filter(|(location, _, _)| *location == "query")
        .map(|(_, name, parameter)| {
            let schema = schema(&parameter["schema"]);
            let schema = Schema {
                upload_target: text(&parameter["x-upload-target"]).or(schema.upload_target),
                ..schema
            };
            live_parameter(
                &mut taken,
                "query",
                name,
                parameter["required"] == true,
                schema,
            )
        })
        .collect();

    let media = resolve(document, &operation["requestBody"])["content"]
        .as_object()
        .and_then(|content| {
            content
                .iter()
                .find(|(media_type, _)| is_json(media_type))
                .map(|(_, media)| media)
        });
    let Some(media) = media else {
        return (query, Vec::new());
    };
    let body = body_schema(document, &media["schema"]);
    let required: Vec<&str> = body["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let fields = body["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, property)| {
            let required = required.contains(&name.as_str());
            live_parameter(&mut taken, "field", name, required, schema(property))
        })
        .collect();
    (query, fields)
}

fn live_parameter(
    taken: &mut HashSet<String>,
    prefix: &str,
    name: &str,
    required: bool,
    schema: Schema,
) -> LiveParameter {
    LiveParameter {
        name: name.to_string(),
        option: claim(taken, name, prefix),
        kind: schema.kind,
        array: schema.array,
        format: schema.format,
        choices: schema.choices,
        required,
        upload_target: schema.upload_target,
    }
}

/// The option name for `name` (`claim` in `src/generated.rs`): its kebab case, or `<prefix>-` before it when that
/// is taken, then a number.
fn claim(taken: &mut HashSet<String>, name: &str, prefix: &str) -> String {
    let wanted = kebab_case(name);
    let wanted = if wanted.is_empty() {
        prefix.to_string()
    } else {
        wanted
    };
    if taken.insert(wanted.clone()) {
        return wanted;
    }
    let mut long = format!("{prefix}-{wanted}");
    let mut number = 2;
    while !taken.insert(long.clone()) {
        long = format!("{prefix}-{wanted}-{number}");
        number += 1;
    }
    long
}

/// The body schema with `$ref` followed and a nullable `oneOf` taken as its one shape (`body_schema` in
/// `src/openapi.rs`).
fn body_schema<'a>(document: &'a Value, schema: &'a Value) -> &'a Value {
    let mut schema = schema;
    for _ in 0..MAX_REF_DEPTH {
        schema = resolve(document, schema);
        let Some(variants) = schema["oneOf"]
            .as_array()
            .or_else(|| schema["anyOf"].as_array())
        else {
            break;
        };
        let mut shapes = variants.iter().filter(|variant| variant["type"] != "null");
        match (shapes.next(), shapes.next()) {
            (Some(shape), None) => schema = shape,
            _ => break,
        }
    }
    schema
}

/// What `schema` in `src/openapi.rs` reads from a schema.
struct Schema {
    kind: Kind,
    array: bool,
    format: Option<String>,
    choices: Vec<String>,
    upload_target: Option<String>,
}

fn schema(schema: &Value) -> Schema {
    let upload_target = text(&schema["x-upload-target"]);
    let format = text(&schema["format"]);
    let types = types(schema);
    let is_json = schema.get("$ref").is_some()
        || ["oneOf", "anyOf", "allOf"]
            .iter()
            .any(|key| schema.get(*key).is_some());
    if !is_json && types.contains(&"array") {
        return Schema {
            array: true,
            upload_target,
            ..self::schema(&schema["items"])
        };
    }
    let kind = if is_json || types.contains(&"object") {
        Kind::Json
    } else if types.contains(&"integer") {
        Kind::Integer
    } else if types.contains(&"number") {
        Kind::Number
    } else if types.contains(&"boolean") {
        Kind::Boolean
    } else if types.contains(&"string") {
        Kind::String
    } else {
        Kind::Json
    };
    let choices = schema["enum"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|choice| match (kind, choice) {
            (Kind::String, Value::String(text)) => Some(text.clone()),
            (Kind::Integer | Kind::Number, Value::Number(number)) => Some(number.to_string()),
            _ => None,
        })
        .collect();
    Schema {
        kind,
        array: false,
        format,
        choices,
        upload_target,
    }
}

/// `type` as a list, without `null`.
fn types(schema: &Value) -> Vec<&str> {
    match &schema["type"] {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| *name != "null")
            .collect(),
        _ => Vec::new(),
    }
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
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
