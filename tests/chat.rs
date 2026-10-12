mod support;

use std::collections::VecDeque;
use std::sync::Mutex;

use serde_json::{Value, json};
use support::{
    FakeServer, Recorded, Reply, THREADS_PATH, description_with_chat_and_uploads_moved,
    mvdm_with_token, serving, without_operations,
};

const CREATED: &str = "c0ffee00-0000-4000-8000-000000000001";
const SPLIT: &str = "5b1170ff-0000-4000-8000-000000000002";

/// Plays the three conversation operations at the paths the description gives them (under `/v2/threads`, not
/// `/api/v1/conversations`). The send answers `send`; each read answers the next of `reads`.
fn conversations(send: Reply, reads: Vec<Value>) -> FakeServer {
    let send = Mutex::new(Some(send));
    let reads = Mutex::new(VecDeque::from(reads));
    FakeServer::start(serving(
        description_with_chat_and_uploads_moved(),
        move |request: &Recorded| match (request.method.as_str(), request.path()) {
            ("POST", THREADS_PATH) => Reply::json(
                201,
                &json!({ "id": CREATED, "turnState": "idle", "lastError": null }).to_string(),
            ),
            ("POST", path) if path.ends_with("/messages") => {
                send.lock().unwrap().take().expect("one send per test")
            }
            ("GET", path) if path.ends_with("/messages") => {
                let read = reads.lock().unwrap().pop_front().expect("no more reads");
                Reply::json(200, &read.to_string())
            }
            _ => Reply::problem(404, "{\"status\":404}"),
        },
    ))
}

fn sent(conversation_id: &str, seq: i64) -> Reply {
    let body = json!({
        "account": { "id": 1, "name": "Alpha" },
        "messageId": "m-sent",
        "conversationId": conversation_id,
        "seq": seq,
        "queued": false,
    });
    Reply::json(202, &body.to_string())
}

fn read(conversation_id: &str, turn_state: &str, items: Vec<Value>) -> Value {
    json!({
        "account": { "id": 1, "name": "Alpha" },
        "conversationId": conversation_id,
        "turnState": turn_state,
        "lastError": null,
        "items": items,
    })
}

fn message(seq: i64, text: &str) -> Value {
    json!({
        "id": format!("m{seq}"),
        "seq": seq,
        "role": "assistant",
        "sender": null,
        "text": text,
        "attachments": [],
        "toolActivity": [{ "name": "query_entities", "outcome": "ok", "isWrite": false }],
        "turnState": "running",
        "createdAt": "2026-09-25T10:00:00Z",
    })
}

fn chat(server: &FakeServer, args: &[&str]) -> support::Run {
    mvdm_with_token(server, &[&["compliance", "chat", "send"], args].concat())
}

fn calls(server: &FakeServer) -> Vec<String> {
    server
        .api_requests()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url))
        .collect()
}

#[test]
fn without_a_conversation_it_starts_one_sends_and_reads_until_idle() {
    let reply = message(4, "Hello, how can I help?");
    let server = conversations(
        sent(CREATED, 3),
        vec![read(CREATED, "idle", vec![reply.clone()])],
    );

    let run = chat(&server, &["hi"]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        json!({
            "conversationId": CREATED,
            "lastError": null,
            "queued": false,
            "messages": [reply],
        })
    );
    assert_eq!(
        calls(&server),
        [
            "POST /v2/threads".to_string(),
            format!("POST /v2/threads/{CREATED}/messages"),
            format!("GET /v2/threads/{CREATED}/messages?after=3&wait=60"),
        ]
    );
    let requests = server.api_requests();
    assert_eq!(requests[0].body, b"{}");
    assert_eq!(requests[1].body, b"{\"text\":\"hi\"}");
    assert!(
        requests
            .iter()
            .all(|request| request.header("Authorization") == Some("Bearer cmp_pat_test"))
    );
}

#[test]
fn a_reply_over_several_reads_prints_every_new_message_once_in_order() {
    let first = message(8, "Looking at your risks.");
    let second = message(9, "Shall I accept risk R-3?");
    let server = conversations(
        sent(CREATED, 7),
        vec![
            read(CREATED, "running", vec![first.clone()]),
            read(CREATED, "running", vec![]),
            read(CREATED, "idle", vec![second.clone()]),
        ],
    );

    let run = chat(&server, &["check my risks", "--conversation", CREATED]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["messages"], json!([first, second]));
    let reads: Vec<String> = calls(&server)
        .into_iter()
        .filter(|call| call.starts_with("GET"))
        .collect();
    assert_eq!(
        reads,
        [
            format!("GET /v2/threads/{CREATED}/messages?after=7&wait=60"),
            format!("GET /v2/threads/{CREATED}/messages?after=8&wait=60"),
            format!("GET /v2/threads/{CREATED}/messages?after=8&wait=60"),
        ]
    );
}

#[test]
fn with_a_conversation_none_is_started() {
    let server = conversations(sent(CREATED, 1), vec![read(CREATED, "idle", vec![])]);

    let run = chat(&server, &["yes, go ahead", "--conversation", CREATED]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        calls(&server),
        [
            format!("POST /v2/threads/{CREATED}/messages"),
            format!("GET /v2/threads/{CREATED}/messages?after=1&wait=60"),
        ]
    );
}

#[test]
fn a_topic_split_conversation_is_the_one_read_and_printed() {
    let reply = message(2, "A new topic, so a new conversation.");
    let server = conversations(
        sent(SPLIT, 1),
        vec![read(SPLIT, "idle", vec![reply.clone()])],
    );

    let run = chat(&server, &["something else", "--conversation", CREATED]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["conversationId"], SPLIT);
    assert_eq!(run.stdout_json()["messages"], json!([reply]));
    assert_eq!(
        calls(&server).last().unwrap(),
        &format!("GET /v2/threads/{SPLIT}/messages?after=1&wait=60")
    );
}

#[test]
fn an_exhausted_allowance_exits_1_with_the_problem_details() {
    let problem = "{\"type\":\"allowance-exhausted\",\"title\":\"The Assistant allowance is used up.\",\"status\":402}";
    let server = conversations(Reply::problem(402, problem), vec![]);

    let run = chat(&server, &["hi", "--conversation", CREATED]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr.trim_end(), problem);
    assert_eq!(server.api_requests().len(), 1);
}

#[test]
fn a_conflict_exits_1_with_the_problem_details() {
    let problem =
        "{\"type\":\"conflict\",\"title\":\"The conversation is closed.\",\"status\":409}";
    let server = conversations(Reply::problem(409, problem), vec![]);

    let run = chat(&server, &["hi", "--conversation", CREATED]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr.trim_end(), problem);
}

#[test]
fn a_browser_handoff_exits_3() {
    let problem = json!({
        "type": "browser-handoff",
        "title": "Finish this in the browser.",
        "status": 403,
        "handoffUrl": "https://compliance.example/billing",
    });
    let server = conversations(Reply::problem(403, &problem.to_string()), vec![]);

    let run = chat(&server, &["buy more", "--conversation", CREATED]);

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        json!({
            "status": "browser_handoff",
            "reason": "Finish this in the browser.",
            "handoffUrl": "https://compliance.example/billing",
        })
    );
}

#[test]
fn a_rate_limited_read_is_retried() {
    let reply = message(2, "Done.");
    let limited = Mutex::new(true);
    let idle = read(CREATED, "idle", vec![reply.clone()]).to_string();
    let server = FakeServer::start(serving(
        description_with_chat_and_uploads_moved(),
        move |request: &Recorded| match request.method.as_str() {
            "POST" => sent(CREATED, 1),
            _ if std::mem::replace(&mut *limited.lock().unwrap(), false) => {
                Reply::problem(429, "{\"status\":429}").header("Retry-After", "0")
            }
            _ => Reply::json(200, &idle),
        },
    ));

    let run = chat(&server, &["hi", "--conversation", CREATED]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["messages"], json!([reply]));
    assert_eq!(server.api_requests().len(), 3);
}

#[test]
fn chat_send_reads_the_description_once_and_calls_the_paths_it_gives() {
    let server = conversations(sent(CREATED, 1), vec![read(CREATED, "idle", vec![])]);

    let run = chat(&server, &["hi"]);

    assert_eq!(run.code, 0, "{run:#?}");
    let fetches = server.description_fetches();
    assert_eq!(fetches, 1);
    // None of the fixture's own paths is called.
    assert!(
        server
            .api_requests()
            .iter()
            .all(|request| request.path().starts_with(THREADS_PATH))
    );
}

#[test]
fn a_product_whose_description_lacks_a_conversation_operation_has_no_chat() {
    for missing in [
        "conversations.create",
        "conversations.send-message",
        "conversations.list-messages",
    ] {
        let server = FakeServer::start(serving(
            without_operations(description_with_chat_and_uploads_moved(), &[missing]),
            |_: &Recorded| Reply::json(200, "{}"),
        ));

        let help = mvdm_with_token(&server, &["compliance", "--help"]);
        let run = chat(&server, &["hi"]);

        assert_eq!(help.code, 0, "{missing}: {help:#?}");
        assert!(!help.stdout.contains("chat"), "{missing}: {}", help.stdout);
        assert_eq!(run.code, 2, "{missing}: {run:#?}");
        assert_eq!(run.stderr_json()["error"], "usage", "{missing}");
        assert!(server.api_requests().is_empty(), "{missing}");
    }
}

#[test]
fn a_product_whose_description_holds_the_conversation_operations_lists_chat() {
    let server = conversations(sent(CREATED, 1), vec![]);

    let help = mvdm_with_token(&server, &["compliance", "--help"]);

    assert_eq!(help.code, 0, "{help:#?}");
    assert!(help.stdout.contains("chat"), "{}", help.stdout);
    assert!(server.api_requests().is_empty());
}

#[test]
fn chat_send_with_the_description_down_is_a_network_error() {
    let server = FakeServer::start(|_: &Recorded| Reply::empty(503));

    let run = chat(&server, &["hi"]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["error"], "network");
}

#[test]
fn a_queued_message_says_so() {
    let body = json!({ "conversationId": CREATED, "seq": 5, "queued": true });
    let server = conversations(
        Reply::json(202, &body.to_string()),
        vec![read(CREATED, "idle", vec![])],
    );

    let run = chat(&server, &["and mine?", "--conversation", CREATED]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["queued"], true);
}
