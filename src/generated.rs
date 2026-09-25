//! The commands built from the API description, beside the hand-written ones.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::PathBuf;

use clap::builder::{PossibleValuesParser, TypedValueParser, ValueParser};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Number, Value};

use crate::openapi::{Kind, Operation, Parameter, Schema, kebab_case};

/// Option names every generated command keeps for itself.
const GLOBAL_OPTIONS: [&str; 4] = ["help", "body", "out", "file"];
pub const BODY: &str = "body";
pub const OUT: &str = "out";
pub const FILE: &str = "file";

pub fn path_id(parameter: &Parameter) -> String {
    format!("path:{}", parameter.name)
}

pub fn query_id(parameter: &Parameter) -> String {
    format!("query:{}", parameter.name)
}

pub fn field_id(parameter: &Parameter) -> String {
    format!("field:{}", parameter.name)
}

/// Where the upload id `--file` fills goes.
#[derive(Clone, Copy, PartialEq)]
pub enum Location {
    Query,
    Body,
}

/// The parameter `--file` fills: the first query parameter or body field with an upload target.
pub struct UploadSlot<'a> {
    pub parameter: &'a Parameter,
    pub location: Location,
    pub target: &'a str,
}

impl UploadSlot<'_> {
    pub fn id(&self) -> String {
        match self.location {
            Location::Query => query_id(self.parameter),
            Location::Body => field_id(self.parameter),
        }
    }

    /// Whether `parameter`, found at `location`, is this slot.
    pub fn is(&self, parameter: &Parameter, location: Location) -> bool {
        self.location == location && std::ptr::eq(self.parameter, parameter)
    }
}

pub fn upload_slot(operation: &Operation) -> Option<UploadSlot<'_>> {
    let query = operation
        .query_params
        .iter()
        .map(|parameter| (parameter, Location::Query));
    let fields = operation
        .body
        .iter()
        .flat_map(|body| &body.fields)
        .map(|field| (field, Location::Body));
    query.chain(fields).find_map(|(parameter, location)| {
        Some(UploadSlot {
            parameter,
            location,
            target: parameter.schema.upload_target.as_deref()?,
        })
    })
}

/// Whether the command line can be parsed by the hand-written commands alone: no command, `--version`, or a
/// hand-written command. A group the hand-written commands share with generated ones, such as `accounts`,
/// needs the description unless a hand-written action follows it.
pub fn needs_description(handwritten: &Command, args: &[OsString]) -> bool {
    let Some(first) = token(args, 1) else {
        return false;
    };
    if first == "help" || first == "--help" || first == "-h" {
        return true;
    }
    if first.starts_with('-') {
        return false;
    }
    match handwritten.find_subcommand(first) {
        None => true,
        Some(command) if !command.has_subcommands() => false,
        Some(command) => {
            !token(args, 2).is_some_and(|second| command.find_subcommand(second).is_some())
        }
    }
}

/// Whether the command line names a group or action that neither the hand-written commands nor `operations`
/// know.
pub fn names_unknown_command(
    handwritten: &Command,
    operations: &[Operation],
    args: &[OsString],
) -> bool {
    let Some(first) = token(args, 1).filter(|first| is_name(first)) else {
        return false;
    };
    let command = handwritten.find_subcommand(first);
    if command.is_some_and(|command| !command.has_subcommands()) {
        return false;
    }
    let mut actions = operations
        .iter()
        .filter(|operation| operation.group == first)
        .peekable();
    if command.is_none() && actions.peek().is_none() {
        return true;
    }
    let Some(second) = token(args, 2).filter(|second| is_name(second)) else {
        return false;
    };
    let handwritten_action =
        command.is_some_and(|command| command.find_subcommand(second).is_some());
    !handwritten_action && !actions.any(|operation| operation.action == second)
}

fn token(args: &[OsString], index: usize) -> Option<&str> {
    args.get(index).and_then(|arg| arg.to_str())
}

fn is_name(token: &str) -> bool {
    !token.starts_with('-') && token != "help"
}

/// The hand-written commands with every generated group and action added, and the operations reachable
/// through them. A hand-written command wins over a generated one of the same name: a hand-written command
/// without actions hides the whole group, and a hand-written action hides only that action.
pub fn tree(handwritten: Command, operations: &[Operation]) -> (Command, Vec<&Operation>) {
    let mut groups: BTreeMap<&str, Vec<&Operation>> = BTreeMap::new();
    for operation in operations {
        groups.entry(&operation.group).or_default().push(operation);
    }

    let mut root = handwritten;
    let mut reachable = Vec::new();
    for (group, members) in groups {
        let existing = root.find_subcommand(group);
        if group == "help" || existing.is_some_and(|command| !command.has_subcommands()) {
            continue;
        }
        let members: Vec<&Operation> = members
            .into_iter()
            .filter(|operation| {
                operation.action != "help"
                    && !existing
                        .is_some_and(|command| command.find_subcommand(&operation.action).is_some())
            })
            .collect();
        let actions: Vec<Command> = members.iter().map(|operation| action(operation)).collect();
        reachable.extend(members.iter().copied());
        root = if existing.is_some() {
            root.mut_subcommand(group, |command| command.subcommands(actions))
        } else {
            root.subcommand(group_command(group, &members).subcommands(actions))
        };
    }
    (root, reachable)
}

/// The operation a parsed command line names, with the matches of its action.
pub fn operation_for<'a, 'm>(
    matches: &'m ArgMatches,
    reachable: &[&'a Operation],
) -> Option<(&'a Operation, &'m ArgMatches)> {
    let (group, group_matches) = matches.subcommand()?;
    let (action, action_matches) = group_matches.subcommand()?;
    reachable
        .iter()
        .find(|operation| operation.is_named(group, action))
        .map(|operation| (*operation, action_matches))
}

fn group_command(group: &str, members: &[&Operation]) -> Command {
    let mut tags: Vec<&str> = Vec::new();
    for tag in members.iter().flat_map(|operation| &operation.tags) {
        if !tags.contains(&tag.as_str()) {
            tags.push(tag);
        }
    }
    let about = if tags.is_empty() {
        "Generated from the API description.".to_string()
    } else {
        format!("Tags: {}.", tags.join(", "))
    };
    Command::new(group.to_string())
        .about(about)
        .subcommand_required(true)
        .disable_help_subcommand(true)
}

/// One operation as a command. Names are claimed in a fixed order: the global options, then query parameters,
/// then body fields. A name already taken gets the prefix `query-` or `field-`, then a number, and its help
/// says so.
fn action(operation: &Operation) -> Command {
    let summary = operation
        .summary
        .clone()
        .unwrap_or_else(|| format!("{} {}", operation.method, operation.path));
    let long_about = match &operation.description {
        Some(description) => format!("{summary}\n\n{description}"),
        None => summary.clone(),
    };
    let mut command = Command::new(operation.action.clone())
        .about(summary)
        .long_about(long_about);

    for parameter in &operation.path_params {
        command = command.arg(
            Arg::new(path_id(parameter))
                .value_name(parameter.name.clone())
                .required(true)
                .value_parser(value_parser(&parameter.schema))
                .allow_negative_numbers(is_numeric(&parameter.schema))
                .help(help(
                    parameter,
                    &[format!("Type: {}.", type_name(&parameter.schema))],
                )),
        );
    }

    let upload = upload_slot(operation);
    let mut taken: HashSet<String> = GLOBAL_OPTIONS.iter().map(|name| name.to_string()).collect();
    for parameter in &operation.query_params {
        let (long, mut notes) = claim(&mut taken, &parameter.name, "query");
        let arg = option(query_id(parameter), long.clone(), &parameter.schema);
        let slot = upload
            .as_ref()
            .filter(|slot| slot.is(parameter, Location::Query));
        let arg = if let Some(slot) = slot {
            command = command.arg(file_option(slot, &long, &mut notes));
            if parameter.required {
                arg.required_unless_present(FILE)
            } else {
                arg
            }
        } else {
            arg.required(parameter.required)
        };
        command = command.arg(arg.help(help(parameter, &notes)));
    }
    if let Some(body) = &operation.body {
        for field in &body.fields {
            let (long, mut notes) = claim(&mut taken, &field.name, "field");
            if field.required {
                notes.push("Required in the body.".to_string());
            }
            if let Some(target) = &field.schema.upload_target {
                notes.push(format!("An upload id for the upload target `{target}`."));
            }
            if let Some(slot) = upload
                .as_ref()
                .filter(|slot| slot.is(field, Location::Body))
            {
                command = command.arg(file_option(slot, &long, &mut notes));
            }
            let arg = option(field_id(field), long, &field.schema);
            command = command.arg(arg.help(help(field, &notes)));
        }
        command = command.arg(
            Arg::new(BODY)
                .long(BODY)
                .value_name("JSON|@FILE")
                .help("The whole JSON body, inline or read from a file with @path. Field options win over the same keys in it."),
        );
    }
    command.arg(
        Arg::new(OUT)
            .long(OUT)
            .value_name("PATH")
            .value_parser(clap::value_parser!(PathBuf))
            .required(operation.download)
            .help(if operation.download {
                "Write the downloaded file here and print a summary. Required: the answer is not JSON."
            } else {
                "Write the response body to this file and print a summary instead."
            }),
    )
}

/// `--file`, which uploads a file and sends its upload id in place of `--<long>`, and the note `--<long>` gets.
fn file_option(slot: &UploadSlot, long: &str, notes: &mut Vec<String>) -> Arg {
    notes.push(format!("Or pass --{FILE}."));
    let mut help = format!(
        "Upload this file to the upload target `{}` and send its upload id as `{}`, in place of --{long}.",
        slot.target, slot.parameter.name
    );
    let arg = Arg::new(FILE)
        .long(FILE)
        .value_name("PATH")
        .value_parser(clap::value_parser!(PathBuf))
        .conflicts_with(slot.id());
    let arg = if slot.parameter.schema.array {
        help.push_str(" Repeat it for more files: each becomes one id, in order.");
        arg.action(ArgAction::Append)
    } else {
        arg
    };
    arg.help(help)
}

/// The option name for `name`, and a note when it had to differ.
fn claim(taken: &mut HashSet<String>, name: &str, prefix: &str) -> (String, Vec<String>) {
    let wanted = kebab_case(name);
    let wanted = if wanted.is_empty() {
        prefix.to_string()
    } else {
        wanted
    };
    if taken.insert(wanted.clone()) {
        return (wanted, Vec::new());
    }
    let mut long = format!("{prefix}-{wanted}");
    let mut number = 2;
    while !taken.insert(long.clone()) {
        long = format!("{prefix}-{wanted}-{number}");
        number += 1;
    }
    let note = format!("Sends `{name}`; named --{long} because --{wanted} is taken.");
    (long, vec![note])
}

fn option(id: String, long: String, schema: &Schema) -> Arg {
    let arg = Arg::new(id)
        .long(long)
        .value_name(schema.kind.name().to_ascii_uppercase())
        .value_parser(value_parser(schema))
        .allow_negative_numbers(is_numeric(schema));
    if schema.array {
        arg.action(ArgAction::Append)
    } else {
        arg
    }
}

fn is_numeric(schema: &Schema) -> bool {
    matches!(schema.kind, Kind::Integer | Kind::Number)
}

fn help(parameter: &Parameter, notes: &[String]) -> String {
    let mut parts: Vec<String> = parameter.description.iter().cloned().collect();
    if let Some(format) = &parameter.schema.format {
        parts.push(format!("Format: {format}."));
    }
    if parameter.schema.array {
        parts.push("Repeat the option for more values.".to_string());
    }
    parts.extend(notes.iter().cloned());
    parts.join(" ")
}

fn type_name(schema: &Schema) -> String {
    let name = schema.kind.name();
    if schema.array {
        format!("list of {name}")
    } else {
        name.to_string()
    }
}

/// Parses an argument into the JSON value it sends, so a wrong type is a usage mistake before any request.
fn value_parser(schema: &Schema) -> ValueParser {
    match schema.kind {
        Kind::String if !schema.choices.is_empty() => {
            ValueParser::new(PossibleValuesParser::new(schema.choices.clone()).map(Value::String))
        }
        Kind::String => {
            ValueParser::new(|text: &str| Ok::<_, String>(Value::String(text.to_string())))
        }
        Kind::Integer | Kind::Number if !schema.choices.is_empty() => ValueParser::new(
            PossibleValuesParser::new(schema.choices.clone()).map(|text| {
                number(&text)
                    .map(Value::Number)
                    .unwrap_or(Value::String(text))
            }),
        ),
        Kind::Integer => ValueParser::new(|text: &str| {
            number(text)
                .filter(|number| number.is_i64() || number.is_u64())
                .map(Value::Number)
                .ok_or_else(|| format!("{text} is not an integer, such as 5"))
        }),
        Kind::Number => ValueParser::new(|text: &str| {
            number(text)
                .map(Value::Number)
                .ok_or_else(|| format!("{text} is not a number, such as 2.5"))
        }),
        Kind::Boolean => ValueParser::new(
            PossibleValuesParser::new(["true", "false"]).map(|text| Value::Bool(text == "true")),
        ),
        Kind::Json => ValueParser::new(|text: &str| {
            serde_json::from_str::<Value>(text)
                .map_err(|error| format!("expected JSON text: {error}"))
        }),
    }
}

fn number(text: &str) -> Option<Number> {
    text.trim().parse::<Number>().ok()
}
