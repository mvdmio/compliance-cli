mod api;
mod cli;
mod config;
mod failure;
mod http;
mod output;
mod response;

use std::process::ExitCode;

use clap::Parser;
use clap::error::ErrorKind;

use cli::{Cli, Command};
use failure::Failure;

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return parse_failure(error),
    };

    let result = match cli.command {
        Command::Api(args) => api::run(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => failure.report(),
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
