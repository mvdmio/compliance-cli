//! One command group per Product, found through Auth's Product list or an `MVDM_<NAME>_URL` variable.

mod support;

use std::time::Duration;

use serde_json::Value;
use support::{
    DESCRIPTION_PATH, FakeServer, LIST_PATH, Recorded, Reply, Run, age_files, auth_metadata,
    closed_address, fixture, listed_product, mvdm, product_list, serving, store_sign_in, unix_now,
    without_operations,
};
use tempfile::TempDir;

/// The bearer the stored sign-in sends.
const BEARER: &str = "Bearer stored-access";

/// A fake Product that serves the fixture description and answers every other request with an empty page.
fn product() -> FakeServer {
    FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            Reply::json(200, &fixture().to_string())
        } else {
            Reply::json(200, "{\"items\":[]}")
        }
    })
}

/// A fake Auth that lists `products` in order, with a sign-in stored for it in a config folder of its own.
struct Rig {
    auth: FakeServer,
    config: TempDir,
}

impl Rig {
    fn start(products: Vec<Value>) -> Self {
        let auth = FakeServer::start(move |request| {
            auth_metadata(request)
                .or_else(|| product_list(request, &products))
                .unwrap_or_else(|| Reply::empty(404))
        });
        let config = tempfile::tempdir().expect("a config folder");
        store_sign_in(
            config.path(),
            &auth.url(),
            "stored-access",
            unix_now() + 3600,
            "stored-refresh",
        );
        Rig { auth, config }
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let auth = self.auth.url();
        let config = self.config.path().to_string_lossy().into_owned();
        let cache = self
            .config
            .path()
            .join("cache")
            .to_string_lossy()
            .into_owned();
        let mut all = vec![
            ("MVDM_AUTH_URL", auth.as_str()),
            ("MVDM_CONFIG_DIR", config.as_str()),
            ("MVDM_CACHE_DIR", cache.as_str()),
        ];
        all.extend_from_slice(env);
        mvdm(args, &all)
    }

    fn list_fetches(&self) -> usize {
        self.auth
            .requests()
            .iter()
            .filter(|request| request.path() == LIST_PATH)
            .count()
    }
}

#[test]
fn the_top_level_help_lists_every_listed_product_with_its_display_name() {
    let (one, two) = (product(), product());
    let rig = Rig::start(vec![
        listed_product("one", &one.url()),
        listed_product("two", &two.url()),
    ]);

    let run = rig.run(&["--help"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let commands: Vec<&str> = run
        .stdout
        .lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .collect();
    let line = |name: &str| {
        commands
            .iter()
            .position(|line| line.trim_start().starts_with(name))
            .unwrap_or_else(|| panic!("{name}: {}", run.stdout))
    };
    assert!(commands[line("one")].contains("ONE"), "{}", run.stdout);
    assert!(commands[line("two")].contains("TWO"), "{}", run.stdout);
    assert!(line("one") < line("two"), "{}", run.stdout);
    // Listing the Products reads no Product.
    assert!(one.requests().is_empty() && two.requests().is_empty());
}

#[test]
fn a_variable_only_product_follows_the_listed_ones_in_the_help() {
    let one = product();
    let rig = Rig::start(vec![listed_product("one", &one.url())]);

    let run = rig.run(
        &["--help"],
        &[
            ("MVDM_ZED_URL", "http://zed.example.test"),
            ("MVDM_ALPHA_URL", "http://alpha.example.test"),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    let position = |name: &str| {
        run.stdout
            .find(&format!("\n  {name} "))
            .unwrap_or_else(|| panic!("{name}: {}", run.stdout))
    };
    assert!(position("one") < position("alpha"), "{}", run.stdout);
    assert!(position("alpha") < position("zed"), "{}", run.stdout);
}

#[test]
fn a_product_help_lists_its_generated_commands_and_ends_with_its_guides() {
    let one = product();
    let rig = Rig::start(vec![listed_product("one", &one.url())]);

    let run = rig.run(&["one", "--help"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    for expected in ["ONE", "risks", "frameworks", "api", "chat"] {
        assert!(run.stdout.contains(expected), "{expected}: {}", run.stdout);
    }
    let docs = format!("Docs: {}/agents", one.url());
    assert!(run.stdout.trim_end().ends_with(&docs), "{}", run.stdout);
}

#[test]
fn a_listed_product_without_guides_ends_its_help_with_the_default_guides_address() {
    let one = product();
    let mut listed = listed_product("one", &one.url());
    listed.as_object_mut().expect("an object").remove("guides");
    let rig = Rig::start(vec![listed]);

    let run = rig.run(&["one", "--help"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let docs = format!("Docs: {}/agents", one.url());
    assert!(run.stdout.trim_end().ends_with(&docs), "{}", run.stdout);
}

#[test]
fn each_product_group_reaches_its_own_product_with_the_one_stored_sign_in() {
    let (one, two) = (product(), product());
    let rig = Rig::start(vec![
        listed_product("one", &one.url()),
        listed_product("two", &two.url()),
    ]);

    let first = rig.run(&["one", "risks", "list"], &[]);
    let second = rig.run(&["two", "risks", "list"], &[]);

    assert_eq!(first.code, 0, "{first:#?}");
    assert_eq!(second.code, 0, "{second:#?}");
    for server in [&one, &two] {
        let requests = server.api_requests();
        assert_eq!(requests.len(), 1, "{requests:#?}");
        assert_eq!(requests[0].path(), "/api/v1/risks");
        assert_eq!(requests[0].header("Authorization"), Some(BEARER));
        assert_eq!(server.description_fetches(), 1);
    }
}

#[test]
fn raw_calls_and_chat_go_to_the_named_product_only() {
    let (one, two) = (product(), product());
    let rig = Rig::start(vec![
        listed_product("one", &one.url()),
        listed_product("two", &two.url()),
    ]);

    let raw = rig.run(&["two", "api", "GET", "/api/v1/risks"], &[]);
    let chat = rig.run(&["two", "chat", "send", "hi", "--conversation", "c1"], &[]);

    assert_eq!(raw.code, 0, "{raw:#?}");
    // The fake answers a page, not a message: the request reached the Product, then the answer was refused.
    assert_eq!(chat.stderr_json()["error"], "invalid-response", "{chat:#?}");
    assert!(one.api_requests().is_empty());
    assert_eq!(two.api_requests()[0].path(), "/api/v1/risks");
    assert_eq!(
        two.api_requests()[1].path(),
        "/api/v1/conversations/c1/messages"
    );
    // `chat` exists only because the description holds its operations, so it was read; `api` reads nothing.
    assert_eq!(two.description_fetches(), 1);
}

#[test]
fn chat_is_in_only_the_group_of_a_product_whose_description_holds_its_operations() {
    let one = product();
    let without_chat = without_operations(
        fixture(),
        &[
            "conversations.create",
            "conversations.send-message",
            "conversations.list-messages",
        ],
    );
    let two = FakeServer::start(serving(without_chat, |_: &Recorded| {
        Reply::json(200, "{\"items\":[]}")
    }));
    let rig = Rig::start(vec![
        listed_product("one", &one.url()),
        listed_product("two", &two.url()),
    ]);

    let help_one = rig.run(&["one", "--help"], &[]);
    let help_two = rig.run(&["two", "--help"], &[]);
    let chat_two = rig.run(&["two", "chat", "send", "hi"], &[]);

    assert_eq!(help_one.code, 0, "{help_one:#?}");
    assert!(help_one.stdout.contains("chat"), "{}", help_one.stdout);
    assert_eq!(help_two.code, 0, "{help_two:#?}");
    assert!(!help_two.stdout.contains("chat"), "{}", help_two.stdout);
    assert_eq!(chat_two.code, 2, "{chat_two:#?}");
    assert_eq!(chat_two.stderr_json()["error"], "usage");
    assert!(two.api_requests().is_empty());
}

#[test]
fn an_unknown_product_refetches_the_list_past_a_fresh_cache() {
    let (one, two) = (product(), product());
    let listed = std::sync::Arc::new(std::sync::Mutex::new(vec![listed_product(
        "one",
        &one.url(),
    )]));
    let served = std::sync::Arc::clone(&listed);
    let auth = FakeServer::start(move |request| {
        auth_metadata(request)
            .or_else(|| product_list(request, &served.lock().unwrap()))
            .unwrap_or_else(|| Reply::empty(404))
    });
    let config = tempfile::tempdir().expect("a config folder");
    store_sign_in(
        config.path(),
        &auth.url(),
        "stored-access",
        unix_now() + 3600,
        "stored-refresh",
    );
    let folder = config.path().to_string_lossy().into_owned();
    let cache = config.path().join("cache").to_string_lossy().into_owned();
    let auth_url = auth.url();
    let env = [
        ("MVDM_AUTH_URL", auth_url.as_str()),
        ("MVDM_CONFIG_DIR", folder.as_str()),
        ("MVDM_CACHE_DIR", cache.as_str()),
    ];
    let fetches = || {
        auth.requests()
            .iter()
            .filter(|request| request.path() == LIST_PATH)
            .count()
    };

    assert_eq!(mvdm(&["one", "risks", "list"], &env).code, 0);
    assert_eq!(fetches(), 1);

    // The cached list is fresh, and `two` is not in it.
    listed
        .lock()
        .unwrap()
        .push(listed_product("two", &two.url()));
    let run = mvdm(&["two", "risks", "list"], &env);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(fetches(), 2);
    assert_eq!(two.api_requests().len(), 1);

    // A name the fresh list still lacks is a usage mistake that names the Products, after one fetch.
    let missing = mvdm(&["nonesuch", "risks", "list"], &env);

    assert_eq!(missing.code, 2, "{missing:#?}");
    assert_eq!(missing.stderr_json()["error"], "usage");
    let message = missing.stderr_json()["message"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        message.contains("nonesuch") && message.contains("one, two"),
        "{message}"
    );
    assert_eq!(fetches(), 3);
}

#[test]
fn a_product_named_like_a_top_level_command_is_hidden_by_it() {
    let (hidden, help, one) = (product(), product(), product());
    let rig = Rig::start(vec![
        listed_product("skill", &hidden.url()),
        listed_product("help", &help.url()),
        listed_product("one", &one.url()),
    ]);

    let skill = rig.run(&["skill"], &[]);
    let listing = rig.run(&["--help"], &[]);

    // `skill` prints the built-in Markdown, not a Product's help, and reads no Product.
    assert_eq!(skill.code, 0, "{skill:#?}");
    assert!(serde_json::from_str::<Value>(&skill.stdout).is_err());
    assert!(hidden.requests().is_empty() && help.requests().is_empty());
    // The help lists the top-level command once, under its own text, and not a Product named `help`.
    assert_eq!(listing.code, 0, "{listing:#?}");
    let lines_for = |name: &str| {
        let prefix = format!("  {name} ");
        listing
            .stdout
            .lines()
            .filter(|line| line.starts_with(&prefix))
            .count()
    };
    assert_eq!(lines_for("skill"), 1, "{}", listing.stdout);
    assert!(!listing.stdout.contains("SKILL"), "{}", listing.stdout);
    assert_eq!(lines_for("help"), 1, "{}", listing.stdout);
    assert!(!listing.stdout.contains("HELP"), "{}", listing.stdout);
    assert_eq!(lines_for("one"), 1, "{}", listing.stdout);
}

#[test]
fn the_flat_commands_are_gone() {
    let one = product();
    let rig = Rig::start(vec![listed_product("one", &one.url())]);

    for args in [
        vec!["risks", "list"],
        vec!["api", "GET", "/api/v1/risks"],
        vec!["chat", "send", "hi"],
    ] {
        let run = rig.run(&args, &[]);

        assert_eq!(run.code, 2, "{args:?}: {run:#?}");
        assert_eq!(run.stderr_json()["error"], "usage");
    }
    assert!(one.api_requests().is_empty());
}

#[test]
fn a_variable_reaches_its_product_with_auth_down() {
    let server = product();
    let auth = closed_address();
    let config = tempfile::tempdir().expect("a config folder");
    let folder = config.path().to_string_lossy().into_owned();
    let cache = config.path().join("cache").to_string_lossy().into_owned();

    let run = mvdm(
        &["compliance", "risks", "list"],
        &[
            ("MVDM_AUTH_URL", &auth),
            ("MVDM_CONFIG_DIR", &folder),
            ("MVDM_CACHE_DIR", &cache),
            ("MVDM_COMPLIANCE_URL", &server.url()),
            ("MVDM_TOKEN", "cmp_pat_test"),
        ],
    );
    let help = mvdm(
        &["compliance", "--help"],
        &[
            ("MVDM_AUTH_URL", &auth),
            ("MVDM_CONFIG_DIR", &folder),
            ("MVDM_CACHE_DIR", &cache),
            ("MVDM_COMPLIANCE_URL", &server.url()),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        server.api_requests()[0].header("Authorization"),
        Some("Bearer cmp_pat_test")
    );
    let docs = format!("Docs: {}/agents", server.url());
    assert!(help.stdout.trim_end().ends_with(&docs), "{}", help.stdout);
}

#[test]
fn a_failed_list_fetch_fails_only_a_product_no_variable_names() {
    let server = product();
    let auth = closed_address();
    let config = tempfile::tempdir().expect("a config folder");
    let folder = config.path().to_string_lossy().into_owned();
    let cache = config.path().join("cache").to_string_lossy().into_owned();
    let env = [
        ("MVDM_AUTH_URL", auth.as_str()),
        ("MVDM_CONFIG_DIR", folder.as_str()),
        ("MVDM_CACHE_DIR", cache.as_str()),
        ("MVDM_TOKEN", "cmp_pat_test"),
    ];
    let url = server.url();
    let named = [env.as_slice(), &[("MVDM_COMPLIANCE_URL", url.as_str())]].concat();

    let by_variable = mvdm(&["compliance", "risks", "list"], &named);
    let by_list = mvdm(&["other", "risks", "list"], &named);

    assert_eq!(by_variable.code, 0, "{by_variable:#?}");
    assert_eq!(by_list.code, 1, "{by_list:#?}");
    assert_eq!(by_list.stderr_json()["error"], "discovery");
}

#[test]
fn a_variable_product_keeps_the_display_name_the_list_gives_it() {
    let listed = product();
    let local = product();
    let rig = Rig::start(vec![listed_product("one", &listed.url())]);
    let env = [("MVDM_ONE_URL", local.url())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();

    // The top-level help reads the list; the group's help then finds it cached.
    let top = rig.run(&["--help"], &env);
    let group = rig.run(&["one", "--help"], &env);
    age_files(
        &rig.config.path().join("cache"),
        Duration::from_secs(5 * 3600),
    );
    let stale = rig.run(&["one", "--help"], &env);

    assert_eq!(top.code, 0, "{top:#?}");
    assert!(group.stdout.contains("ONE"), "{}", group.stdout);
    assert!(stale.stdout.contains("ONE"), "{}", stale.stdout);
    // The variable's host serves the Product; the list's host is never read.
    assert!(listed.requests().is_empty());
    assert_eq!(rig.list_fetches(), 1);
}

#[test]
fn a_browser_handoff_from_a_product_group_exits_3_and_prints_its_link() {
    let server = FakeServer::start(|request| {
        if request.path() == DESCRIPTION_PATH {
            return Reply::json(200, &fixture().to_string());
        }
        Reply::problem(
            403,
            "{\"type\":\"browser-handoff\",\"detail\":\"Confirm.\",\"handoffUrl\":\"https://example.test/go\"}",
        )
    });
    let rig = Rig::start(vec![listed_product("one", &server.url())]);

    let run = rig.run(&["one", "risks", "list"], &[]);

    assert_eq!(run.code, 3, "{run:#?}");
    assert_eq!(run.stdout_json()["handoffUrl"], "https://example.test/go");
}
