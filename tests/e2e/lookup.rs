//! What the Sweeps share: finding the read command at a path (`read_at`, for both), and, for the Read sweep, running
//! a command once however often it is asked for and finding the values a command needs (path values, query values)
//! in the lists the Test-bed prints.

use std::collections::HashMap;

use serde_json::Value;

use crate::operations::{LiveOperation, placeholders};
use crate::support::Run;
use crate::test_bed::Scenario;

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

/// The read in `operations` at `path` that the CLI has a command for.
pub fn read_at<'a>(operations: &'a [LiveOperation], path: &str) -> Option<&'a LiveOperation> {
    operations.iter().find(|operation| {
        operation.is_read() && operation.path == path && operation.command.is_some()
    })
}

/// How one command went. A skipped command depends on an empty list.
pub enum Verdict {
    Passed,
    Skipped(String),
    Failed(String),
}

/// Every command a Sweep ran, so a list that finds ids runs once, as its own read too.
pub struct Lookup<'a> {
    pub operations: &'a [LiveOperation],
    scenario: &'a Scenario,
    runs: HashMap<Vec<String>, Run>,
}

impl<'a> Lookup<'a> {
    pub fn new(scenario: &'a Scenario, operations: &'a [LiveOperation]) -> Self {
        Lookup {
            operations,
            scenario,
            runs: HashMap::new(),
        }
    }

    /// Runs `compliance <args>` once; a second call with the same arguments answers with the first run.
    pub fn run(&mut self, args: Vec<String>) -> &Run {
        let scenario = self.scenario;
        self.runs.entry(args).or_insert_with_key(|args| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            scenario.run(&args)
        })
    }

    /// A value for every placeholder in `path`, in order, each from the first item of the list it depends on.
    pub fn path_values(&mut self, path: &str) -> Result<Vec<String>, Verdict> {
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

    /// The option and value a read at `path` needs although the description does not mark it required: empty when
    /// it needs none.
    pub fn query_args(&mut self, path: &str) -> Result<Vec<String>, Verdict> {
        let Some(query) = QUERY_SOURCES.iter().find(|query| query.read == path) else {
            return Ok(Vec::new());
        };
        let value = self.first_value(query.source.list, query.source.field, &[])?;
        Ok(vec![query.option.to_string(), value])
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
        let Some(command) = read_at(operations, list).and_then(|read| read.command.as_ref()) else {
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
