use serde_json::Value;
use ureq::http::Method;

use crate::response::is_json;

const MAX_REF_DEPTH: usize = 16;

const METHODS: [(&str, Method); 7] = [
    ("get", Method::GET),
    ("put", Method::PUT),
    ("post", Method::POST),
    ("delete", Method::DELETE),
    ("options", Method::OPTIONS),
    ("head", Method::HEAD),
    ("patch", Method::PATCH),
];

pub struct Operation {
    pub group: String,
    pub action: String,
    pub method: Method,
    pub path: String,
    pub tags: Vec<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub path_params: Vec<Parameter>,
    pub query_params: Vec<Parameter>,
    pub body: Option<Body>,
    pub download: bool,
}

impl Operation {
    pub fn is_named(&self, group: &str, action: &str) -> bool {
        self.group == group && self.action == action
    }
}

pub struct Parameter {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    pub schema: Schema,
}

pub struct Body {
    pub required: bool,
    pub fields: Vec<Parameter>,
}

pub struct Schema {
    pub kind: Kind,
    pub array: bool,
    pub format: Option<String>,
    pub choices: Vec<String>,
    pub upload_target: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    String,
    Integer,
    Number,
    Boolean,
    Json,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::String => "string",
            Kind::Integer => "integer",
            Kind::Number => "number",
            Kind::Boolean => "boolean",
            Kind::Json => "JSON",
        }
    }
}

pub fn operations(document: &Value) -> Vec<Operation> {
    let mut operations: Vec<Operation> = Vec::new();
    let Some(paths) = document["paths"].as_object() else {
        return operations;
    };
    for (path, item) in paths {
        let item = resolve(document, item);
        for (key, method) in &METHODS {
            let Some(operation) = item.get(*key) else {
                continue;
            };
            let Some(operation) = read_operation(document, path, item, method, operation) else {
                continue;
            };
            let taken = operations
                .iter()
                .any(|existing| existing.is_named(&operation.group, &operation.action));
            if !taken {
                operations.push(operation);
            }
        }
    }
    operations
}

fn read_operation(
    document: &Value,
    path: &str,
    item: &Value,
    method: &Method,
    operation: &Value,
) -> Option<Operation> {
    let (group, action) = operation["operationId"].as_str()?.split_once('.')?;
    let (group, action) = (kebab_case(group), kebab_case(action));
    if group.is_empty() || action.is_empty() {
        return None;
    }

    let mut path_params = Vec::new();
    let mut query_params = Vec::new();
    for (location, parameter) in parameters(document, item, operation) {
        match location.as_str() {
            "path" => path_params.push(parameter),
            "query" => query_params.push(parameter),
            _ => {}
        }
    }
    let order = placeholders(path);
    path_params.retain(|parameter| order.contains(&parameter.name.as_str()));
    path_params.sort_by_key(|parameter| order.iter().position(|name| *name == parameter.name));

    Some(Operation {
        group,
        action,
        method: method.clone(),
        path: path.to_string(),
        tags: operation["tags"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tag| tag.as_str().map(str::to_string))
            .collect(),
        summary: text(&operation["summary"]),
        description: text(&operation["description"]),
        path_params,
        query_params,
        body: body(document, &operation["requestBody"]),
        download: is_download(document, &operation["responses"]),
    })
}

fn parameters(document: &Value, item: &Value, operation: &Value) -> Vec<(String, Parameter)> {
    let mut found: Vec<(String, Parameter)> = Vec::new();
    let listed = item["parameters"]
        .as_array()
        .into_iter()
        .chain(operation["parameters"].as_array())
        .flatten();
    for parameter in listed {
        let parameter = resolve(document, parameter);
        let (Some(name), Some(location)) = (parameter["name"].as_str(), parameter["in"].as_str())
        else {
            continue;
        };
        found.retain(|(existing, known)| !(existing == location && known.name == name));
        let schema = schema(&parameter["schema"]);
        found.push((
            location.to_string(),
            Parameter {
                name: name.to_string(),
                description: text(&parameter["description"]),
                required: parameter["required"] == true,
                schema: Schema {
                    upload_target: text(&parameter["x-upload-target"]).or(schema.upload_target),
                    ..schema
                },
            },
        ));
    }
    found
}

fn placeholders(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
        .collect()
}

fn body(document: &Value, request_body: &Value) -> Option<Body> {
    let request_body = resolve(document, request_body);
    let content = request_body["content"].as_object()?;
    let media = content
        .iter()
        .find(|(media_type, _)| is_json(media_type))
        .map(|(_, media)| media)?;
    let schema = body_schema(document, &media["schema"]);
    let required: Vec<&str> = schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let fields = schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, property)| Parameter {
            name: name.clone(),
            description: text(&property["description"]),
            required: required.contains(&name.as_str()),
            schema: self::schema(property),
        })
        .collect();
    Some(Body {
        required: request_body["required"] == true,
        fields,
    })
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

fn schema(schema: &Value) -> Schema {
    let upload_target = text(&schema["x-upload-target"]);
    let format = text(&schema["format"]);
    let types = types(schema);
    let is_json = schema.get("$ref").is_some()
        || ["oneOf", "anyOf", "allOf"]
            .iter()
            .any(|key| schema.get(*key).is_some());
    if !is_json && types.contains(&"array") {
        let items = self::schema(&schema["items"]);
        return Schema {
            array: true,
            upload_target,
            ..items
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

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub fn kebab_case(name: &str) -> String {
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
