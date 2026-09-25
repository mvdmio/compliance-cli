use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line tool for the Compliance REST API. Every command prints JSON.
///
/// Exit codes: 0 success, 1 error, 2 usage mistake, 3 Browser handoff (a person must finish the act in the
/// browser at `handoffUrl`).
#[derive(Parser)]
#[command(name = "compliance", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Make one raw call to the REST API with the current credential.
    Api(ApiArgs),
}

#[derive(Args)]
pub struct ApiArgs {
    /// HTTP method, such as GET, POST, PUT, PATCH, or DELETE.
    pub method: String,

    /// Path relative to the host, such as /api/v1/risks. A query string rides in the path.
    pub path: String,

    /// JSON request body, given inline or read from a file with @path.
    #[arg(long, value_name = "JSON|@FILE")]
    pub body: Option<String>,

    /// Write the response body to this file and print a summary instead.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}
