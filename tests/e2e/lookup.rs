use std::collections::HashMap;

use serde_json::Value;

use crate::operations::{LiveOperation, placeholders};
use crate::support::Run;
use crate::test_bed::Scenario;

struct Source {
    list: &'static str,
    field: &'static str,
}

struct PathSource {
    read: &'static str,
    source: Source,
}

struct QuerySource {
    read: &'static str,
    option: &'static str,
    source: Source,
}

const PATH_SOURCES: [PathSource; 3] = [
    PathSource {
        read: "/api/v1/check-runs/{id}",
        source: Source {
            list: "/api/v1/check-results",
            field: "checkRunId",
        },
    },
    PathSource {
        read: "/api/v1/frameworks/{frameworkId}/requirements/{code}",
        source: Source {
            list: "/api/v1/frameworks/{frameworkId}/requirements",
            field: "code",
        },
    },
    PathSource {
        read: "/api/v1/conversations/{id}",
        source: Source {
            list: "/api/v1/conversations",
            field: "id",
        },
    },
];

const QUERY_SOURCES: [QuerySource; 1] = [QuerySource {
    read: "/api/v1/invitations/preview",
    option: "--person-ids",
    source: Source {
        list: "/api/v1/people",
        field: "id",
    },
}];

pub fn read_at<'a>(operations: &'a [LiveOperation], path: &str) -> Option<&'a LiveOperation> {
    operations.iter().find(|operation| {
        operation.is_read() && operation.path == path && operation.command.is_some()
    })
}

pub enum Verdict {
    Passed,
    Skipped(String),
    Failed(String),
}

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

    pub fn run(&mut self, args: Vec<String>) -> &Run {
        let scenario = self.scenario;
        self.runs.entry(args).or_insert_with_key(|args| {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            scenario.run(&args)
        })
    }

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

    pub fn query_args(&mut self, path: &str) -> Result<Vec<String>, Verdict> {
        let Some(query) = QUERY_SOURCES.iter().find(|query| query.read == path) else {
            return Ok(Vec::new());
        };
        let value = self.first_value(query.source.list, query.source.field, &[])?;
        Ok(vec![query.option.to_string(), value])
    }

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
