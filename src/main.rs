mod accounts;
mod api;
mod cache;
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
mod products;
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
use clap::{ArgMatches, CommandFactory, FromArgMatches};
use serde_json::Value;

use cli::{ChatGroup, Cli, Command, ProductCommand};
use failure::Failure;
use generated::Target;
use openapi::Operation;
use products::Product;

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();
    match generated::target(&Cli::command(), &args) {
        Target::TopLevel => parse_and_run(Cli::command(), None, &args),
        Target::Help => top_level_help(&args),
        Target::Product(name) => match products::named(name) {
            Ok(product) => product_group(&product, &args),
            Err(failure) => failure.report(),
        },
    }
}

/// `mvdm --help`: the top-level commands and the Products. A list of Products that cannot be read leaves the
/// Products a variable names, and the help says what is missing.
fn top_level_help(args: &[OsString]) -> ExitCode {
    let (listed, failure) = products::for_help();
    let mut root = Cli::command();
    for product in listed {
        if root.find_subcommand(&product.name).is_none() && product.name != "help" {
            root = root.subcommand(clap::Command::new(product.name).about(product.display_name));
        }
    }
    root = root.after_help(match failure {
        None => "Run `mvdm <product> --help` to list a Product's commands.".to_string(),
        Some(Failure::Local { message, .. }) => {
            format!("The list of Products could not be read from Auth: {message}")
        }
        Some(_) => "The list of Products could not be read from Auth.".to_string(),
    });
    parse_and_run(root, None, args)
}

/// What a Product's OpenAPI description gives its group: the generated operations, and the two sets of operations
/// the hand-written commands call, found by operation ID. A hand-written command exists only when its operations do.
#[derive(Default)]
struct Found {
    operations: Vec<Operation>,
    /// The conversation operations, which `chat send` calls.
    chat: Option<chat::Operations>,
    /// `uploads.create`, which requests the Upload links for `--file`.
    uploads: Option<Operation>,
}

impl Found {
    fn read(document: &Value) -> Self {
        Found {
            operations: openapi::operations(document),
            chat: chat::operations(document),
            uploads: openapi::operation_by_id(document, upload::CREATE),
        }
    }

    /// `base`, the commands every group has, with `chat` when the description holds its operations.
    fn handwritten(&self, base: &clap::Command) -> clap::Command {
        if self.chat.is_some() {
            cli::with_chat(base.clone())
        } else {
            base.clone()
        }
    }
}

/// The Product a command line named, and what its description gave the group.
struct Group<'a> {
    product: &'a Product,
    found: &'a Found,
    /// The generated operations of `found` reachable in the group.
    reachable: &'a [&'a Operation],
}

/// `mvdm <product> …`: the Product's group, built from its OpenAPI description.
fn product_group(product: &Product, args: &[OsString]) -> ExitCode {
    let rest = &args[2..];
    let docs_line = format!("Docs: {}", product.guides);
    let base = cli::product_command(product);
    if !generated::needs_description(&base, rest) {
        let root = Cli::command().subcommand(base.after_help(docs_line));
        let group = Group {
            product,
            found: &Found::default(),
            reachable: &[],
        };
        return parse_and_run(root, Some(group), args);
    }
    let (found, after_help) = match description::load(product) {
        Ok(mut description) => {
            let mut found = Found::read(&description.value);
            if generated::names_unknown_command(&found.handwritten(&base), &found.operations, rest)
            {
                description.refetch(|| description::fetch(product));
                found = Found::read(&description.value);
            }
            (found, docs_line)
        }
        // Help and the hand-written commands still work without the description; help says what is missing.
        Err(_) if !generated::names_unknown_command(&base, &[], rest) => (
            Found::default(),
            format!(
                "The commands generated from {} are missing: it could not be read.\n\n{docs_line}",
                product.open_api
            ),
        ),
        Err(failure) => return failure.report(),
    };
    let handwritten = found.handwritten(&base).after_help(after_help);
    let (command, reachable) =
        generated::tree(handwritten, &found.operations, found.uploads.as_ref());
    let root = Cli::command().subcommand(command);
    let group = Group {
        product,
        found: &found,
        reachable: &reachable,
    };
    parse_and_run(root, Some(group), args)
}

/// Parses `args` with `root` and runs the command. `group` is set when the command line named a Product.
fn parse_and_run(root: clap::Command, group: Option<Group>, args: &[OsString]) -> ExitCode {
    let matches = match root.try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => return parse_failure(error),
    };
    let result = match group {
        Some(group) => run_in_group(&group, &matches),
        None => Cli::from_arg_matches(&matches).map_err(usage).and_then(run),
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
        Command::Skill => skill::run(),
    }
}

/// Runs the command inside `product`'s group: a generated one, or `api` or `chat`.
fn run_in_group(group: &Group, matches: &ArgMatches) -> Result<(), Failure> {
    let product = group.product;
    let Some((_, group_matches)) = matches.subcommand() else {
        return Err(Failure::Usage(format!(
            "Give a command for {}.",
            product.name
        )));
    };
    if let Some((operation, matches)) = generated::operation_for(group_matches, group.reachable) {
        return dispatch::run(product, group.found.uploads.as_ref(), operation, matches);
    }
    // `chat` is in the group only when `group.found.chat` holds its operations.
    if let Some(operations) = &group.found.chat
        && group_matches.subcommand_name() == Some("chat")
    {
        let ChatGroup::Chat(command) = ChatGroup::from_arg_matches(group_matches).map_err(usage)?;
        return chat::run(product, operations, command);
    }
    match ProductCommand::from_arg_matches(group_matches).map_err(usage)? {
        ProductCommand::Api(args) => api::run(product, args),
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
        _ => usage(error).report(),
    }
}

fn usage(error: clap::Error) -> Failure {
    let rendered = error.render().to_string();
    Failure::Usage(rendered.trim().trim_start_matches("error: ").to_string())
}
