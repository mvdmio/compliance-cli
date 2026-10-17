use serde_json::{Value, json};
use ureq::http::Method;

use crate::cli::{ChatCommand, ChatSendArgs};
use crate::failure::Failure;
use crate::http::Client;
use crate::output;
use crate::request::path_segment;
use crate::response;

const CONVERSATIONS: &str = "/api/v1/conversations";
/// The longest the API holds a message read for a running turn.
const WAIT_SECONDS: u32 = 60;

/// `compliance chat send`.
pub fn run(command: ChatCommand) -> Result<(), Failure> {
    match command {
        ChatCommand::Send(args) => send(args),
    }
}

fn send(args: ChatSendArgs) -> Result<(), Failure> {
    let mut client = Client::signed_in()?;

    let requested_id = match args.conversation {
        Some(id) => id,
        None => {
            let created = post(&mut client, CONVERSATIONS, &json!({}))?;
            required_string(&created, "id")?
        }
    };

    let sent = post(
        &mut client,
        &messages_path(&requested_id),
        &json!({ "text": args.text }),
    )?;
    // Topic Split may land the message in a new conversation; that one holds the reply.
    let conversation_id = required_string(&sent, "conversationId")?;
    let mut after = seq(&sent)?;
    // A shared conversation's running turn holds a queued message for the turn after it, which this read may end
    // before.
    let queued = sent["queued"] == true;

    let read_path = messages_path(&conversation_id);
    let mut messages = Vec::new();
    loop {
        let page = response::read_json(client.send(
            &Method::GET,
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

fn post(client: &mut Client, path: &str, body: &Value) -> Result<Value, Failure> {
    let body = body.to_string();
    response::read_json(client.send(&Method::POST, path, Some(body.as_bytes()))?)
}

fn messages_path(conversation_id: &str) -> String {
    format!("{CONVERSATIONS}/{}/messages", path_segment(conversation_id))
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
