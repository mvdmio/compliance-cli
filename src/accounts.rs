use serde_json::Value;

use crate::cli::AccountsCommand;
use crate::description;
use crate::failure::{ErrorCode, Failure};
use crate::http::Client;
use crate::openapi::{self, Operation};
use crate::products::{self, Product};
use crate::request::fill_placeholder;
use crate::response;

const LIST: &str = "accounts.list";
const SWITCH: &str = "accounts.switch";

/// `mvdm accounts list` and `mvdm accounts switch <id>`.
pub fn run(command: AccountsCommand) -> Result<(), Failure> {
    let product = products::account_product()?;
    let mut client = Client::signed_in(&product)?;
    let answer = match command {
        AccountsCommand::List => {
            let list = find(&product, LIST)?;
            client.send(&list.method, &list.path, None)?
        }
        AccountsCommand::Switch { id } => {
            let switch = find(&product, SWITCH)?;
            let [parameter] = switch.path_params.as_slice() else {
                return Err(unusable(
                    &product,
                    SWITCH,
                    "it must have exactly one path parameter",
                ));
            };
            let path = fill_placeholder(&switch.path, &parameter.name, &id.to_string());
            client.send(&switch.method, &path, None)?
        }
    };
    response::print_response(answer, None)
}

/// The Account the credential acts in, or `null` when it acts in none.
pub fn current(product: &Product, client: &mut Client) -> Result<Value, Failure> {
    let list = find(product, LIST)?;
    let list = response::read_json(client.send(&list.method, &list.path, None)?)?;
    let current = list["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["current"] == true))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(current)
}

/// The operation `id` in the Product's description. A description without it is fetched once more; if it still
/// lacks it, the failure names the Product and the operation.
fn find(product: &Product, id: &str) -> Result<Operation, Failure> {
    let mut description = description::load(product)?;
    if let Some(operation) = openapi::operation_by_id(&description.value, id) {
        return Ok(operation);
    }
    description.refetch(|| description::fetch(product));
    openapi::operation_by_id(&description.value, id)
        .ok_or_else(|| unusable(product, id, "its API description has no such operation"))
}

fn unusable(product: &Product, id: &str, why: &str) -> Failure {
    Failure::local(
        ErrorCode::Discovery,
        format!(
            "Cannot use {id} of {}: {why} ({}).",
            product.name, product.open_api
        ),
    )
}
