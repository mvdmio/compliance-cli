use serde_json::Value;
use ureq::http::Method;

use crate::cli::AccountsCommand;
use crate::failure::Failure;
use crate::http::Client;
use crate::response;

const PATH: &str = "/api/v1/accounts";

/// `compliance accounts list` and `compliance accounts switch <id>`.
pub fn run(command: AccountsCommand) -> Result<(), Failure> {
    let mut client = Client::signed_in()?;
    let answer = match command {
        AccountsCommand::List => client.send(&Method::GET, PATH, None)?,
        AccountsCommand::Switch { id } => {
            client.send(&Method::POST, &format!("{PATH}/{id}/switch"), None)?
        }
    };
    response::print_response(answer, None)
}

/// The Account the credential acts in, or `null` when it acts in none.
pub fn current(client: &mut Client) -> Result<Value, Failure> {
    let list = response::read_json(client.send(&Method::GET, PATH, None)?)?;
    let current = list["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["current"] == true))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(current)
}
