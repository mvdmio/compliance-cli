use serde_json::{Value, json};

use crate::cli::{ChatCommand, ChatSendArgs};
use crate::failure::Failure;
use crate::http::Client;
use crate::openapi::{self, Operation};
use crate::output;
use crate::products::Product;
use crate::request::fill_placeholder;
use crate::response;

/// The longest the API holds a message read for a running turn.
const WAIT_SECONDS: u32 = 60;

/// The three operations of a Product's OpenAPI document that `chat send` calls, found by operation ID.
pub struct Operations {
    create: Operation,
    send_message: Operation,
    list_messages: Operation,
}

/// The conversation operations of `document`, or `None` when it lacks one of them: a Product without them has no
/// `chat`. The two that take a conversation need exactly one path parameter, which the conversation id fills.
pub fn operations(document: &Value) -> Option<Operations> {
    let find = |id: &str| openapi::operation_by_id(document, id);
    let operations = Operations {
        create: find("conversations.create")?,
        send_message: find("conversations.send-message")?,
        list_messages: find("conversations.list-messages")?,
    };
    (operations.send_message.path_params.len() == 1
        && operations.list_messages.path_params.len() == 1)
        .then_some(operations)
}

/// `mvdm <product> chat send`.
pub fn run(
    product: &Product,
    operations: &Operations,
    command: ChatCommand,
) -> Result<(), Failure> {
    match command {
        ChatCommand::Send(args) => send(product, operations, args),
    }
}

fn send(product: &Product, operations: &Operations, args: ChatSendArgs) -> Result<(), Failure> {
    let mut client = Client::signed_in(product)?;

    let requested_id = match args.conversation {
        Some(id) => id,
        None => {
            let create = &operations.create;
            let created = call(&mut client, create, &create.path, &json!({}))?;
            required_string(&created, "id")?
        }
    };

    let send_message = &operations.send_message;
    let sent = call(
        &mut client,
        send_message,
        &conversation_path(send_message, &requested_id),
        &json!({ "text": args.text }),
    )?;
    // Topic Split may land the message in a new conversation; that one holds the reply.
    let conversation_id = required_string(&sent, "conversationId")?;
    let mut after = seq(&sent)?;
    // A shared conversation's running turn holds a queued message for the turn after it, which this read may end
    // before.
    let queued = sent["queued"] == true;

    let list_messages = &operations.list_messages;
    let read_path = conversation_path(list_messages, &conversation_id);
    let mut messages = Vec::new();
    loop {
        let page = response::read_json(client.send(
            &list_messages.method,
            &format!("{read_path}?after={after}&wait={WAIT_SECONDS}"),
            None,
        )?)?;
        let items = page["items"]
            .as_array()
            .ok_or_else(|| invalid_response("items"))?;
        if let Some(last) = items.last() {
            after = seq(last)?;
        }
        messages.extend(items.iter().cloned());
        let turn_state = page["turnState"]
            .as_str()
            .ok_or_else(|| invalid_response("turnState"))?;
        if turn_state == "idle" {
            output::print_json(&json!({
                "conversationId": conversation_id,
                "lastError": page["lastError"],
                "queued": queued,
                "messages": messages,
            }));
            return Ok(());
        }
    }
}

/// Calls `operation` at `path` with a JSON body.
fn call(
    client: &mut Client,
    operation: &Operation,
    path: &str,
    body: &Value,
) -> Result<Value, Failure> {
    let body = body.to_string();
    response::read_json(client.send(&operation.method, path, Some(body.as_bytes()))?)
}

/// The operation's path with its one path parameter filled by the conversation id.
fn conversation_path(operation: &Operation, conversation_id: &str) -> String {
    fill_placeholder(
        &operation.path,
        &operation.path_params[0].name,
        conversation_id,
    )
}

fn required_string(value: &Value, field: &'static str) -> Result<String, Failure> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid_response(field))
}

fn seq(value: &Value) -> Result<i64, Failure> {
    value["seq"].as_i64().ok_or_else(|| invalid_response("seq"))
}

fn invalid_response(field: &str) -> Failure {
    Failure::invalid_response(format!("The response has no valid `{field}`."))
}
