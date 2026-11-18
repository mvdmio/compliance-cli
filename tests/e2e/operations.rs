#![allow(
    dead_code,
    reason = "shared with tests/generated.rs, which uses only part of it"
)]

use std::collections::HashSet;

use serde_json::Value;
use ureq::http::Method;

pub const GLOBAL_OPTIONS: [&str; 4] = ["help", "body", "out", "file"];

const METHODS: [(&str, Method); 7] = [
    ("get", Method::GET),
    ("put", Method::PUT),
    ("post", Method::POST),
    ("delete", Method::DELETE),
    ("options", Method::OPTIONS),
    ("head", Method::HEAD),
    ("patch", Method::PATCH),
];

const HANDWRITTEN_LEAVES: [&str; 5] = ["login", "logout", "status", "api", "skill"];

const MAX_REF_DEPTH: usize = 16;

#[derive(Clone, PartialEq, Eq)]
pub struct CommandName {
    pub group: String,
    pub action: String,
}

impl CommandName {
    pub fn from_operation_id(operation_id: &str) -> Option<Self> {
        let (group, action) = operation_id.split_once('.')?;
        let (group, action) = (kebab_case(group), kebab_case(action));
        (!group.is_empty() && !action.is_empty()).then_some(CommandName { group, action })
    }

    pub fn is_generated(&self) -> bool {
        self.group != "help"
            && self.action != "help"
            && !HANDWRITTEN_LEAVES.contains(&self.group.as_str())
    }

    pub fn args(&self) -> Vec<String> {
        vec![self.group.clone(), self.action.clone()]
    }
}

impl std::fmt::Display for CommandName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "compliance {} {}", self.group, self.action)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    String,
    Integer,
    Number,
    Boolean,
    Json,
}

pub struct LiveParameter {
    pub name: String,
    pub option: String,
    pub kind: Kind,
    pub array: bool,
    pub format: Option<String>,
    pub choices: Vec<String>,
    pub required: bool,
    pub upload_target: Option<String>,
}

pub struct LiveOperation {
    pub id: String,
    pub command: Option<CommandName>,
    pub method: Method,
    pub path: String,
    pub download: bool,
    pub query: Vec<LiveParameter>,
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

fn options(
    document: &Value,
    item: &Value,
    operation: &Value,
) -> (Vec<LiveParameter>, Vec<LiveParameter>) {
    let mut taken: HashSet<String> = GLOBAL_OPTIONS.iter().map(|name| name.to_string()).collect();
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

pub fn placeholders(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
        .collect()
}

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

fn is_json(content_type: &str) -> bool {
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    essence == "application/json" || essence.ends_with("+json")
}

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
