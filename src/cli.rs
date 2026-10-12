use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::products::Product;

/// Command-line tool for the REST APIs of the mvdm.io Products. Every command prints JSON.
///
/// Exit codes: 0 success, 1 error, 2 usage mistake, 3 Browser handoff (a person must finish the act in the
/// browser at `handoffUrl`).
#[derive(Parser)]
#[command(name = "mvdm", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Sign in through Auth in the browser, or with a device code, and store the sign-in.
    Login(LoginArgs),

    /// End the stored sign-in at Auth and forget it. MVDM_TOKEN is not touched.
    Logout,

    /// Show the Auth address, the credential in use, the User, and the Account commands act in.
    Status,

    /// List the User's Accounts, or move the credential to another one.
    #[command(subcommand)]
    Accounts(AccountsCommand),

    /// Print the Agent skill file: when and how to use this CLI, as Markdown.
    Skill,
}

/// The hand-written commands inside every Product's group, beside the ones generated from its OpenAPI description.
#[derive(Subcommand)]
pub enum ProductCommand {
    /// Make one raw call to the Product's REST API with the current credential.
    Api(ApiArgs),
}

// The hand-written command in the group of a Product whose OpenAPI description holds the conversation operations.
// Not a doc comment: the derive would set it as the group's `about`, which is the Product's display name.
#[derive(Subcommand)]
pub enum ChatGroup {
    /// Talk to the Product's Assistant.
    #[command(subcommand)]
    Chat(ChatCommand),
}

/// The group `mvdm <name> …`, with the hand-written commands in it that need no description.
pub fn product_command(product: &Product) -> clap::Command {
    // The derive sets an `about` of its own from the enum's doc comment, so the display name comes after it.
    ProductCommand::augment_subcommands(clap::Command::new(product.name.clone()))
        .about(product.display_name.clone())
        .subcommand_required(true)
}

/// `group` with `chat` added.
pub fn with_chat(group: clap::Command) -> clap::Command {
    ChatGroup::augment_subcommands(group)
}

#[derive(Args)]
pub struct LoginArgs {
    /// Sign in with a code on another device instead of a browser on this one.
    #[arg(long)]
    pub device: bool,
}

#[derive(Subcommand)]
pub enum AccountsCommand {
    /// List every Account the User belongs to, the current one marked.
    List,

    /// Move the credential into another Account, for every later command.
    Switch {
        /// The Account id, from `mvdm accounts list`.
        id: u64,
    },
}

#[derive(Subcommand)]
pub enum ChatCommand {
    /// Send one message to our Assistant and wait for its reply.
    ///
    /// Prints `{"conversationId","turnState","lastError","queued","messages"}`: every message after the sent one, as the API
    /// gives it. Answer the Assistant's questions and proposals with the next `chat send --conversation
    /// <conversationId>`.
    Send(ChatSendArgs),
}

#[derive(Args)]
pub struct ChatSendArgs {
    /// The message, in markdown.
    pub text: String,

    /// The conversation to continue. Without it, a new conversation starts.
    #[arg(long, value_name = "ID")]
    pub conversation: Option<String>,
}

#[derive(Args)]
pub struct ApiArgs {
    /// HTTP method, such as GET, POST, PUT, PATCH, or DELETE.
    pub method: String,

    /// Path relative to the Product's host, such as /api/v1/risks. A query string rides in the path.
    pub path: String,

    /// JSON request body, given inline or read from a file with @path.
    #[arg(long, value_name = "JSON|@FILE")]
    pub body: Option<String>,

    /// Write the response body to this file and print a summary instead.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}
