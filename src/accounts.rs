use serde_json::Value;
use ureq::http::Method;

use crate::cli::AccountsCommand;
use crate::config;
use crate::credential;
use crate::failure::Failure;
use crate::http::Client;
use crate::response;

const PATH: &str = "/api/v1/accounts";

/// `compliance accounts list` and `compliance accounts switch <id>`.
pub fn run(command: AccountsCommand) -> Result<(), Failure> {
    let host = config::host();
    let mut client = Client::new(host.clone(), credential::require(&host)?);
    let answer = match command {
        AccountsCommand::List => client.send(&Method::GET, PATH, None)?,
        AccountsCommand::Switch { id } => {
            client.send(&Method::POST, &format!("{PATH}/{id}/switch"), None)?
        }
    };
    response::print_response(answer, None)
}

/// The User's Accounts, and among them the one the credential acts in.
pub struct Accounts {
    pub current: Value,
    pub items: Vec<Value>,
}

pub fn fetch(client: &mut Client) -> Result<Accounts, Failure> {
    let list = response::read_json(client.send(&Method::GET, PATH, None)?)?;
    let items = list["items"].as_array().cloned().unwrap_or_default();
    let current = items
        .iter()
        .find(|item| item["current"] == true)
        .cloned()
        .unwrap_or(Value::Null);
    Ok(Accounts { current, items })
}
