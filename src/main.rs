mod accounts;
mod api;
mod chat;
mod cli;
mod config;
mod credential;
mod description;
mod discovery;
mod dispatch;
mod failure;
mod generated;
mod http;
mod login;
mod logout;
mod oauth;
mod openapi;
mod output;
mod request;
mod response;
mod skill;
mod status;
mod store;
mod upload;

use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches};

use cli::{Cli, Command};
use failure::Failure;
use openapi::Operation;

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    let handwritten = Cli::command();
    if !generated::needs_description(&handwritten, &args) {
        return parse_and_run(handwritten, &[], &args);
    }

    let host = config::host();
    let (operations, handwritten) = match description::load(&host) {
        Ok(mut description) => {
            let mut operations = openapi::operations(&description.document);
            if generated::names_unknown_command(&handwritten, &operations, &args) {
                description.refetch(&host);
                operations = openapi::operations(&description.document);
            }
            (operations, handwritten)
        }
        // Help and the hand-written commands still work without the description; help says what is missing.
        Err(_) if !generated::names_unknown_command(&handwritten, &[], &args) => (
            Vec::new(),
            handwritten.after_help(format!(
                "The commands generated from {host}/openapi/v1.json are missing: it could not be read."
            )),
        ),
        Err(failure) => return failure.report(),
    };
    let (tree, reachable) = generated::tree(handwritten, &operations);
    parse_and_run(tree, &reachable, &args)
}

fn parse_and_run(command: clap::Command, reachable: &[&Operation], args: &[OsString]) -> ExitCode {
    let matches = match command.try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => return parse_failure(error),
    };
    let result = match generated::operation_for(&matches, reachable) {
        Some((operation, matches)) => dispatch::run(operation, matches),
        None => match Cli::from_arg_matches(&matches) {
            Ok(cli) => run(cli),
            Err(error) => return parse_failure(error),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
    }
}

fn run(cli: Cli) -> Result<(), Failure> {
    match cli.command {
        Command::Login(args) => login::run(args),
        Command::Logout => logout::run(),
        Command::Status => status::run(),
        Command::Accounts(command) => accounts::run(command),
        Command::Chat(command) => chat::run(command),
        Command::Api(args) => api::run(args),
        Command::Skill => skill::run(),
    }
}

/// `--help` and `--version` stay plain text; every other parser error is a usage mistake.
fn parse_failure(error: clap::Error) -> ExitCode {
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            // Ignored: a closed stdout leaves nothing to report to.
            let _ = error.print();
            ExitCode::SUCCESS
        }
        _ => {
            let rendered = error.render().to_string();
            let message = rendered.trim().trim_start_matches("error: ").to_string();
            Failure::Usage(message).report()
        }
    }
}
