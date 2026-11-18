use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "compliance",
    version,
    about = "Command-line tool for the Compliance REST API. Every command prints JSON",
    long_about = "Command-line tool for the Compliance REST API. Every command prints JSON.\n\nExit codes: 0 success, 1 error, 2 usage mistake, 3 Browser handoff (a person must finish the act in the browser at `handoffUrl`)."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(
        about = "Sign in through Auth in the browser, or with a device code, and store the sign-in"
    )]
    Login(LoginArgs),

    #[command(
        about = "End the stored sign-in at Auth and forget it. COMPLIANCE_TOKEN is not touched"
    )]
    Logout,

    #[command(
        about = "Show the host, the credential in use, the User, and the Account commands act in"
    )]
    Status,

    #[command(
        subcommand,
        about = "List the User's Accounts, or move the credential to another one"
    )]
    Accounts(AccountsCommand),

    #[command(subcommand, about = "Talk to our Assistant")]
    Chat(ChatCommand),

    #[command(about = "Make one raw call to the REST API with the current credential")]
    Api(ApiArgs),

    #[command(about = "Print the Agent skill file: when and how to use this CLI, as Markdown")]
    Skill,
}

#[derive(Args)]
pub struct LoginArgs {
    #[arg(
        long,
        help = "Sign in with a code on another device instead of a browser on this one"
    )]
    pub device: bool,
}

#[derive(Subcommand)]
pub enum AccountsCommand {
    #[command(about = "List every Account the User belongs to, the current one marked")]
    List,

    #[command(about = "Move the credential into another Account, for every later command")]
    Switch {
        #[arg(help = "The Account id, from `compliance accounts list`")]
        id: u64,
    },
}

#[derive(Subcommand)]
pub enum ChatCommand {
    #[command(
        about = "Send one message to our Assistant and wait for its reply",
        long_about = "Send one message to our Assistant and wait for its reply.\n\nPrints `{\"conversationId\",\"turnState\",\"lastError\",\"queued\",\"messages\"}`: every message after the sent one, as the API gives it. Answer the Assistant's questions and proposals with the next `chat send --conversation <conversationId>`."
    )]
    Send(ChatSendArgs),
}

#[derive(Args)]
pub struct ChatSendArgs {
    #[arg(help = "The message, in markdown")]
    pub text: String,

    #[arg(
        long,
        value_name = "ID",
        help = "The conversation to continue. Without it, a new conversation starts"
    )]
    pub conversation: Option<String>,
}

#[derive(Args)]
pub struct ApiArgs {
    #[arg(help = "HTTP method, such as GET, POST, PUT, PATCH, or DELETE")]
    pub method: String,

    #[arg(
        help = "Path relative to the host, such as /api/v1/risks. A query string rides in the path"
    )]
    pub path: String,

    #[arg(
        long,
        value_name = "JSON|@FILE",
        help = "JSON request body, given inline or read from a file with @path"
    )]
    pub body: Option<String>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Write the response body to this file and print a summary instead"
    )]
    pub out: Option<PathBuf>,
}
