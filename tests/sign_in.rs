mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use support::{
    ACCOUNTS_PATH, DESCRIPTION_PATH, FakeServer, LIST_PATH, Recorded, Reply, Run, accounts_list,
    age_files, auth_metadata, closed_address, credentials_path, description_with_accounts_moved,
    id_token, listed_product, mvdm, oauth_error, product_list, read_credentials, store_sign_in,
    tokens, unix_now, with_description,
};
use tempfile::TempDir;

const CLIENT_ID: &str = "mvdmio.cli";

/// A fake Auth that serves discovery and a Product list naming a fake Compliance first, and a second Product
/// nobody serves. Also a config folder, with the cache folder inside it.
struct Hosts {
    auth: FakeServer,
    compliance: FakeServer,
    config: TempDir,
    /// While set, Auth answers 503 to the Product list.
    list_down: Arc<AtomicBool>,
}

impl Hosts {
    fn start(
        auth: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
        api: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
    ) -> Self {
        Hosts::start_in(tempfile::tempdir().expect("a config folder"), auth, api)
    }

    fn start_in(
        config: TempDir,
        auth: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
        api: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
    ) -> Self {
        Hosts::start_serving(config, auth, with_description(api))
    }

    /// Like `start`, but `compliance` answers every request itself, the description included.
    fn start_serving(
        config: TempDir,
        auth: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
        compliance: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
    ) -> Self {
        let compliance = FakeServer::start(compliance);
        let list = vec![
            listed_product("compliance", &compliance.url()),
            listed_product("other", "http://other.example.test"),
        ];
        let list_down = Arc::new(AtomicBool::new(false));
        let down = Arc::clone(&list_down);
        let auth = FakeServer::start(move |request| {
            if request.path() == LIST_PATH && down.load(Ordering::SeqCst) {
                return Reply::empty(503);
            }
            auth_metadata(request)
                .or_else(|| product_list(request, &list))
                .unwrap_or_else(|| auth(request))
        });
        Hosts {
            auth,
            compliance,
            config,
            list_down,
        }
    }

    /// The first listed Product's host.
    fn host(&self) -> String {
        self.compliance.url()
    }

    /// The first listed Product's API resource, which every request to Auth names.
    fn resource(&self) -> String {
        format!("{}/api", self.host())
    }

    fn cache(&self) -> std::path::PathBuf {
        self.config.path().join("cache")
    }

    /// Runs `mvdm` against this Auth, with the config and cache folders of this test, and no Product variable.
    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let auth = self.auth.url();
        let config = self.config.path().to_string_lossy().into_owned();
        let cache = self.cache().to_string_lossy().into_owned();
        let mut all = vec![
            ("MVDM_AUTH_URL", auth.as_str()),
            ("MVDM_CONFIG_DIR", config.as_str()),
            ("MVDM_CACHE_DIR", cache.as_str()),
        ];
        all.extend_from_slice(env);
        mvdm(args, &all)
    }

    fn store(&self, access_token: &str, expires_at: u64, refresh_token: &str) {
        store_sign_in(
            self.config.path(),
            &self.auth.url(),
            access_token,
            expires_at,
            refresh_token,
        );
    }

    fn stored(&self) -> Value {
        read_credentials(self.config.path())["hosts"][self.auth.url()].clone()
    }

    fn list_fetches(&self) -> usize {
        self.auth_requests(LIST_PATH).len()
    }

    fn auth_requests(&self, path: &str) -> Vec<Recorded> {
        self.auth
            .requests()
            .into_iter()
            .filter(|request| request.path() == path)
            .collect()
    }

    fn api_requests(&self) -> Vec<Recorded> {
        self.compliance
            .requests()
            .into_iter()
            .filter(|request| request.path().starts_with("/api/"))
            .collect()
    }
}

/// The paths of the API calls `server` got, the description left out.
fn api_paths(server: &FakeServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .map(|request| request.path().to_string())
        .filter(|path| path.starts_with("/api/"))
        .collect()
}

fn stderr_lines(run: &Run) -> Vec<Value> {
    run.stderr
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("stderr line is not JSON ({error}): {run:#?}"))
        })
        .collect()
}

fn device_authorization(interval: u64) -> Reply {
    let body = json!({
        "device_code": "device-1",
        "user_code": "ABCD-EFGH",
        "verification_uri": "https://auth.example.test/connect/verify",
        "verification_uri_complete": "https://auth.example.test/connect/verify?user_code=ABCD-EFGH",
        "expires_in": 600,
        "interval": interval,
    });
    Reply::json(200, &body.to_string())
}

/// Auth for the device flow: the first poll is `authorization_pending`, the second answers `answer`.
fn device_auth(answer: fn() -> Reply) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    let polls = AtomicUsize::new(0);
    move |request| match request.path() {
        "/connect/device" => device_authorization(1),
        "/connect/token" if polls.fetch_add(1, Ordering::SeqCst) == 0 => {
            oauth_error("authorization_pending")
        }
        "/connect/token" => answer(),
        _ => Reply::empty(404),
    }
}

fn signed_in_tokens() -> Reply {
    tokens(
        "access-1",
        "refresh-1",
        Some(&id_token("Ada Lovelace", "ada@example.test")),
    )
}

fn accounts_api(request: &Recorded) -> Reply {
    match request.path() {
        ACCOUNTS_PATH => accounts_list(),
        _ => Reply::empty(404),
    }
}

#[test]
fn login_device_shows_the_code_polls_through_pending_and_stores_the_sign_in() {
    let hosts = Hosts::start(device_auth(signed_in_tokens), accounts_api);

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        stderr_lines(&run),
        vec![json!({
            "status": "device_code",
            "verificationUriComplete": "https://auth.example.test/connect/verify?user_code=ABCD-EFGH",
            "verificationUri": "https://auth.example.test/connect/verify",
            "userCode": "ABCD-EFGH",
            "expiresIn": 600,
        })]
    );
    assert_eq!(
        run.stdout_json(),
        json!({
            "status": "logged_in",
            "auth": hosts.auth.url(),
            "user": { "name": "Ada Lovelace", "email": "ada@example.test" },
            "account": { "id": 2, "name": "Beta", "current": true },
        })
    );

    let device = hosts.auth_requests("/connect/device");
    assert_eq!(device.len(), 1);
    assert_eq!(device[0].form("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(device[0].form("scope").as_deref(), Some("openid"));
    assert_eq!(device[0].form("resource"), Some(hosts.resource()));
    let polls = hosts.auth_requests("/connect/token");
    assert_eq!(polls.len(), 2);
    for poll in &polls {
        assert_eq!(
            poll.form("grant_type").as_deref(),
            Some("urn:ietf:params:oauth:grant-type:device_code")
        );
        assert_eq!(poll.form("device_code").as_deref(), Some("device-1"));
        assert_eq!(poll.form("client_id").as_deref(), Some(CLIENT_ID));
    }

    let stored = hosts.stored();
    assert_eq!(stored["accessToken"], "access-1");
    assert_eq!(stored["refreshToken"], "refresh-1");
    assert_eq!(stored["user"]["email"], "ada@example.test");
    let expires_at = stored["expiresAt"].as_u64().expect("an expiry");
    assert!(expires_at > unix_now() + 3500, "{stored}");
    assert_eq!(
        hosts.api_requests()[0].header("Authorization"),
        Some("Bearer access-1")
    );
}

#[test]
fn login_reads_auths_discovery_and_product_list_and_no_protected_resource_metadata() {
    let hosts = Hosts::start(device_auth(signed_in_tokens), accounts_api);

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let paths = |server: &FakeServer| -> Vec<String> {
        server
            .requests()
            .iter()
            .map(|request| request.path().to_string())
            .collect()
    };
    assert_eq!(
        paths(&hosts.compliance),
        vec![DESCRIPTION_PATH, ACCOUNTS_PATH]
    );
    let auth_paths = paths(&hosts.auth);
    assert!(
        auth_paths.contains(&"/.well-known/openid-configuration".to_string()),
        "{auth_paths:?}"
    );
    assert!(
        auth_paths.contains(&LIST_PATH.to_string()),
        "{auth_paths:?}"
    );
    assert!(
        auth_paths
            .iter()
            .all(|path| !path.contains("oauth-protected-resource")),
        "{auth_paths:?}"
    );
}

#[test]
fn login_stores_one_sign_in_under_the_auth_address_and_logout_removes_it() {
    let hosts = Hosts::start(
        |request| match request.path() {
            "/connect/device" => device_authorization(0),
            "/connect/token" => signed_in_tokens(),
            "/connect/revoke" => Reply::empty(200),
            _ => Reply::empty(404),
        },
        accounts_api,
    );

    let login = hosts.run(&["login", "--device"], &[]);
    assert_eq!(login.code, 0, "{login:#?}");
    let keys = |hosts: &Hosts| -> Vec<String> {
        read_credentials(hosts.config.path())["hosts"]
            .as_object()
            .expect("a hosts object")
            .keys()
            .cloned()
            .collect()
    };
    assert_eq!(keys(&hosts), vec![hosts.auth.url()]);

    // A call to the Product adds no entry for its host.
    let call = hosts.run(&["compliance", "api", "GET", ACCOUNTS_PATH], &[]);
    assert_eq!(call.code, 0, "{call:#?}");
    assert_eq!(keys(&hosts), vec![hosts.auth.url()]);

    let logout = hosts.run(&["logout"], &[]);
    assert_eq!(logout.code, 0, "{logout:#?}");
    assert_eq!(hosts.auth_requests("/connect/revoke").len(), 1);
    assert!(!credentials_path(hosts.config.path()).exists());
}

/// Windows and macOS always have a browser to open.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn login_without_a_display_falls_back_to_the_device_code() {
    let hosts = Hosts::start(device_auth(signed_in_tokens), accounts_api);

    let run = hosts.run(&["login"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(stderr_lines(&run)[0]["status"], "device_code");
    assert_eq!(run.stdout_json()["status"], "logged_in");
}

#[test]
fn login_without_an_id_token_prints_a_null_user() {
    let hosts = Hosts::start(
        device_auth(|| tokens("access-1", "refresh-1", None)),
        accounts_api,
    );

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["user"], Value::Null);
}

#[test]
fn a_denied_device_sign_in_exits_1_and_stores_nothing() {
    let hosts = Hosts::start(device_auth(|| oauth_error("access_denied")), accounts_api);

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    assert_eq!(stderr_lines(&run)[1]["error"], "access-denied");
    assert!(!credentials_path(hosts.config.path()).exists());
}

#[test]
fn an_expired_device_code_exits_1() {
    let hosts = Hosts::start(device_auth(|| oauth_error("expired_token")), accounts_api);

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(stderr_lines(&run)[1]["error"], "expired-token");
}

#[cfg(unix)]
#[test]
fn the_stored_sign_in_is_readable_only_by_its_owner_after_login_and_after_a_refresh() {
    use std::os::unix::fs::PermissionsExt;

    let hosts = Hosts::start(
        |request| match (request.path(), request.form("grant_type").as_deref()) {
            ("/connect/device", _) => device_authorization(0),
            ("/connect/token", Some("refresh_token")) => tokens("access-2", "refresh-2", None),
            ("/connect/token", _) => signed_in_tokens(),
            _ => Reply::empty(404),
        },
        |request| {
            if request.header("Authorization") == Some("Bearer access-1") {
                Reply::problem(401, "{\"type\":\"unauthorized\"}")
            } else {
                accounts_list()
            }
        },
    );
    let mode = || {
        std::fs::metadata(credentials_path(hosts.config.path()))
            .expect("the credentials file")
            .permissions()
            .mode()
            & 0o777
    };

    let login = hosts.run(&["login", "--device"], &[]);
    assert_eq!(login.code, 0, "{login:#?}");
    assert_eq!(mode(), 0o600);

    let call = hosts.run(&["compliance", "api", "GET", ACCOUNTS_PATH], &[]);
    assert_eq!(call.code, 0, "{call:#?}");
    assert_eq!(hosts.stored()["refreshToken"], "refresh-2");
    assert_eq!(mode(), 0o600);
}

/// Auth that answers every refresh with `access-2` and `refresh-2`.
fn rotating_auth(request: &Recorded) -> Reply {
    match request.path() {
        "/connect/token" => tokens("access-2", "refresh-2", None),
        _ => Reply::empty(404),
    }
}

fn ok_api(_: &Recorded) -> Reply {
    Reply::json(200, "{\"items\":[]}")
}

#[test]
fn an_expired_access_token_is_refreshed_before_the_call_and_the_new_refresh_token_is_stored() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now() + 30, "refresh-1");

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let refresh = hosts.auth_requests("/connect/token");
    assert_eq!(refresh.len(), 1);
    assert_eq!(
        refresh[0].form("grant_type").as_deref(),
        Some("refresh_token")
    );
    assert_eq!(
        refresh[0].form("refresh_token").as_deref(),
        Some("refresh-1")
    );
    assert_eq!(refresh[0].form("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(refresh[0].form("resource"), Some(hosts.resource()));
    let calls = hosts.api_requests();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].header("Authorization"), Some("Bearer access-2"));
    let stored = hosts.stored();
    assert_eq!(stored["accessToken"], "access-2");
    assert_eq!(stored["refreshToken"], "refresh-2");
    assert_eq!(
        stored["user"]["name"], "Ada Lovelace",
        "a refresh keeps the User"
    );
}

#[test]
fn a_refresh_needs_only_auths_discovery_not_the_product_list() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now() + 30, "refresh-1");
    hosts.list_down.store(true, Ordering::SeqCst);

    let host = hosts.host();
    let run = hosts.run(
        &["compliance", "api", "GET", "/api/v1/risks"],
        &[("MVDM_COMPLIANCE_URL", &host)],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    let refresh = hosts.auth_requests("/connect/token");
    assert_eq!(refresh.len(), 1);
    assert_eq!(
        refresh[0].form("refresh_token").as_deref(),
        Some("refresh-1")
    );
    assert_eq!(refresh[0].form("resource"), None);
    assert_eq!(
        hosts.api_requests()[0].header("Authorization"),
        Some("Bearer access-2")
    );
    assert_eq!(hosts.stored()["refreshToken"], "refresh-2");
}

#[test]
fn a_fresh_access_token_is_used_without_a_refresh() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(hosts.auth_requests("/connect/token").is_empty());
    assert_eq!(
        hosts.api_requests()[0].header("Authorization"),
        Some("Bearer access-1")
    );
}

#[test]
fn a_401_refreshes_once_and_retries_once() {
    let hosts = Hosts::start(rotating_auth, |request| {
        if request.header("Authorization") == Some("Bearer access-2") {
            Reply::json(200, "{\"ok\":1}")
        } else {
            Reply::problem(401, "{\"type\":\"unauthorized\"}")
        }
    });
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "ok": 1 }));
    assert_eq!(hosts.auth_requests("/connect/token").len(), 1);
    assert_eq!(hosts.api_requests().len(), 2);
}

#[test]
fn a_second_401_after_the_refresh_is_reported() {
    let hosts = Hosts::start(rotating_auth, |_| {
        Reply::problem(401, "{\"type\":\"unauthorized\"}")
    });
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["type"], "unauthorized");
    assert_eq!(hosts.auth_requests("/connect/token").len(), 1);
    assert_eq!(hosts.api_requests().len(), 2);
}

#[test]
fn a_refresh_answered_invalid_grant_removes_only_that_auths_entry_and_asks_for_login() {
    let hosts = Hosts::start(|_| oauth_error("invalid_grant"), ok_api);
    hosts.store("access-1", unix_now(), "refresh-1");
    store_sign_in(
        hosts.config.path(),
        "https://other.example.test",
        "other-access",
        unix_now() + 3600,
        "other-refresh",
    );

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    let error = run.stderr_json();
    assert_eq!(error["error"], "not-signed-in");
    assert!(
        error["message"].as_str().unwrap().contains("mvdm login"),
        "{error}"
    );
    assert!(hosts.api_requests().is_empty());
    let hosts_left = read_credentials(hosts.config.path())["hosts"].clone();
    assert_eq!(
        hosts_left.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["https://other.example.test"]
    );
}

#[test]
fn mvdm_token_wins_over_the_stored_sign_in_and_is_never_refreshed_or_stored() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now(), "refresh-1");

    let host = hosts.host();
    let run = hosts.run(
        &["compliance", "api", "GET", "/api/v1/risks"],
        &[
            ("MVDM_TOKEN", "cmp_pat_test"),
            ("MVDM_COMPLIANCE_URL", &host),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        hosts.api_requests()[0].header("Authorization"),
        Some("Bearer cmp_pat_test")
    );
    assert!(hosts.auth.requests().is_empty());
    assert_eq!(hosts.stored()["accessToken"], "access-1");
}

#[test]
fn a_product_variable_and_a_personal_token_run_a_command_with_no_auth_reachable() {
    let compliance = FakeServer::start(ok_api);
    let auth = closed_address();

    let run = mvdm(
        &["compliance", "api", "GET", "/api/v1/risks"],
        &[
            ("MVDM_COMPLIANCE_URL", &compliance.url()),
            ("MVDM_TOKEN", "cmp_pat_test"),
            ("MVDM_AUTH_URL", &auth),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "items": [] }));
    assert_eq!(compliance.only_request().path(), "/api/v1/risks");
}

#[test]
fn without_a_variable_an_unreachable_product_list_is_a_local_discovery_failure() {
    let auth = closed_address();

    let run = mvdm(
        &["compliance", "api", "GET", "/api/v1/risks"],
        &[("MVDM_TOKEN", "cmp_pat_test"), ("MVDM_AUTH_URL", &auth)],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    let error = run.stderr_json();
    assert_eq!(error["error"], "discovery");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains(&format!("{auth}{LIST_PATH}")),
        "{error}"
    );
}

#[test]
fn the_product_list_is_fetched_once_and_kept_for_an_hour() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    let token = [("MVDM_TOKEN", "cmp_pat_test")];

    assert_eq!(hosts.run(&["accounts", "list"], &token).code, 0);
    assert_eq!(hosts.run(&["accounts", "list"], &token).code, 0);
    assert_eq!(hosts.list_fetches(), 1);

    age_files(&hosts.cache(), Duration::from_secs(2 * 60 * 60));
    assert_eq!(hosts.run(&["accounts", "list"], &token).code, 0);
    assert_eq!(hosts.list_fetches(), 2);
}

#[test]
fn a_stale_product_list_is_used_when_the_fetch_fails() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    let token = [("MVDM_TOKEN", "cmp_pat_test")];
    assert_eq!(hosts.run(&["accounts", "list"], &token).code, 0);
    age_files(&hosts.cache(), Duration::from_secs(2 * 60 * 60));
    hosts.list_down.store(true, Ordering::SeqCst);

    let run = hosts.run(&["accounts", "list"], &token);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(hosts.list_fetches(), 2);
    assert_eq!(hosts.api_requests().len(), 2);
}

#[test]
fn the_account_product_is_the_first_variable_in_name_order_else_the_first_listed() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    let early = FakeServer::start(with_description(accounts_api));
    let late = FakeServer::start(with_description(accounts_api));
    let token = ("MVDM_TOKEN", "cmp_pat_test");

    let by_variable = hosts.run(
        &["accounts", "list"],
        &[
            token,
            ("MVDM_ZEBRA_URL", &late.url()),
            ("MVDM_ALPHA_URL", &early.url()),
        ],
    );
    assert_eq!(by_variable.code, 0, "{by_variable:#?}");
    assert_eq!(api_paths(&early), vec![ACCOUNTS_PATH]);
    assert!(api_paths(&late).is_empty());
    assert!(hosts.compliance.requests().is_empty());
    assert_eq!(hosts.list_fetches(), 0, "a variable needs no list");

    let by_list = hosts.run(&["accounts", "list"], &[token]);
    assert_eq!(by_list.code, 0, "{by_list:#?}");
    assert_eq!(hosts.api_requests().len(), 1);
    assert_eq!(api_paths(&early), vec![ACCOUNTS_PATH]);
}

#[test]
fn status_follows_the_account_product_too() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    let named = FakeServer::start(with_description(accounts_api));

    let run = hosts.run(
        &["status"],
        &[
            ("MVDM_TOKEN", "cmp_pat_test"),
            ("MVDM_HEALTH_CHECK_URL", &named.url()),
        ],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(api_paths(&named), vec![ACCOUNTS_PATH]);
    assert!(hosts.compliance.requests().is_empty());
}

#[test]
fn login_names_the_first_listed_products_resource_even_when_a_variable_names_a_product() {
    let hosts = Hosts::start(device_auth(signed_in_tokens), accounts_api);
    let named = FakeServer::start(with_description(accounts_api));

    let run = hosts.run(&["login", "--device"], &[("MVDM_ALPHA_URL", &named.url())]);

    assert_eq!(run.code, 0, "{run:#?}");
    let device = hosts.auth_requests("/connect/device");
    assert_eq!(device[0].form("resource"), Some(hosts.resource()));
    for poll in hosts.auth_requests("/connect/token") {
        assert_eq!(poll.form("resource"), Some(hosts.resource()));
    }
    assert_eq!(api_paths(&named), vec![ACCOUNTS_PATH]);
}

#[test]
fn an_empty_product_list_is_a_local_discovery_failure_for_login() {
    let auth = FakeServer::start(|request| {
        auth_metadata(request)
            .or_else(|| product_list(request, &[]))
            .unwrap_or_else(|| Reply::empty(404))
    });
    let folder = tempfile::tempdir().expect("a folder");
    let config = folder.path().to_string_lossy().into_owned();
    let cache = folder.path().join("cache").to_string_lossy().into_owned();

    let run = mvdm(
        &["login", "--device"],
        &[
            ("MVDM_AUTH_URL", &auth.url()),
            ("MVDM_CONFIG_DIR", &config),
            ("MVDM_CACHE_DIR", &cache),
        ],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    let error = run.stderr_json();
    assert_eq!(error["error"], "discovery");
    assert!(
        error["message"].as_str().unwrap().contains("empty"),
        "{error}"
    );
    assert!(!credentials_path(folder.path()).exists());
}

#[test]
fn logout_revokes_without_the_product_list() {
    let hosts = Hosts::start(
        |request| match request.path() {
            "/connect/revoke" => Reply::empty(200),
            _ => Reply::empty(404),
        },
        ok_api,
    );
    hosts.store("access-1", unix_now() + 3600, "refresh-1");
    hosts.list_down.store(true, Ordering::SeqCst);

    let run = hosts.run(&["logout"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(hosts.auth_requests("/connect/revoke").len(), 1);
    assert!(!credentials_path(hosts.config.path()).exists());
}

#[test]
fn logout_revokes_the_refresh_token_then_deletes_the_file() {
    let hosts = Hosts::start(
        |request| match request.path() {
            "/connect/revoke" => Reply::empty(200),
            _ => Reply::empty(404),
        },
        ok_api,
    );
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["logout"], &[("MVDM_TOKEN", "cmp_pat_test")]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "status": "logged_out" }));
    let revoke = hosts.auth_requests("/connect/revoke");
    assert_eq!(revoke.len(), 1);
    assert_eq!(revoke[0].method, "POST");
    assert_eq!(revoke[0].form("token").as_deref(), Some("refresh-1"));
    assert_eq!(revoke[0].form("client_id").as_deref(), Some(CLIENT_ID));
    assert!(!credentials_path(hosts.config.path()).exists());
}

#[test]
fn logout_keeps_the_entry_when_auth_refuses_the_revocation() {
    let hosts = Hosts::start(|_| Reply::empty(503), ok_api);
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["logout"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    assert_eq!(hosts.stored()["refreshToken"], "refresh-1");
}

#[test]
fn logout_without_a_stored_sign_in_succeeds_without_a_request() {
    let hosts = Hosts::start(|_| Reply::empty(404), ok_api);

    let run = hosts.run(&["logout"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "status": "logged_out" }));
    assert!(hosts.auth.requests().is_empty());
    assert!(hosts.compliance.requests().is_empty());
}

#[test]
fn status_with_a_personal_token_names_it_and_the_current_account() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);

    let run = hosts.run(&["status"], &[("MVDM_TOKEN", "cmp_pat_test")]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        json!({
            "auth": hosts.auth.url(),
            "signedIn": true,
            "credential": { "kind": "personal-token", "source": "MVDM_TOKEN" },
            "user": null,
            "account": { "id": 2, "name": "Beta", "current": true },
        })
    );
    assert_eq!(
        hosts.api_requests()[0].header("Authorization"),
        Some("Bearer cmp_pat_test")
    );
}

#[test]
fn status_with_a_stored_sign_in_names_it_the_user_and_the_current_account() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["status"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let status = run.stdout_json();
    assert_eq!(
        status["credential"],
        json!({ "kind": "agent-connection", "source": "stored sign-in" })
    );
    assert_eq!(
        status["user"],
        json!({ "name": "Ada Lovelace", "email": "ada@example.test" })
    );
    assert_eq!(status["account"]["id"], 2);
}

#[test]
fn status_without_a_credential_reports_signed_out_and_exits_0() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);

    let run = hosts.run(&["status"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        json!({
            "auth": hosts.auth.url(),
            "signedIn": false,
            "credential": null,
            "user": null,
            "account": null,
        })
    );
    assert!(hosts.compliance.requests().is_empty());
}

#[test]
fn status_with_a_rejected_credential_prints_the_problem_and_exits_1() {
    let problem = "{\"type\":\"unauthorized\",\"title\":\"Unauthorized\",\"status\":401}";
    let hosts = Hosts::start(|_| Reply::empty(404), move |_| Reply::problem(401, problem));

    let run = hosts.run(&["status"], &[("MVDM_TOKEN", "cmp_pat_revoked")]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr.trim_end(), problem);
}

#[test]
fn accounts_list_gets_the_accounts_and_prints_them() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);

    let run = hosts.run(&["accounts", "list"], &[("MVDM_TOKEN", "cmp_pat_test")]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["items"][1]["name"], "Beta");
    let request = &hosts.api_requests()[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, ACCOUNTS_PATH);
    assert_eq!(request.header("Authorization"), Some("Bearer cmp_pat_test"));
}

#[test]
fn accounts_switch_posts_to_the_accounts_switch_and_prints_the_answer() {
    let hosts = Hosts::start(
        |_| Reply::empty(404),
        |_| {
            Reply::json(
                200,
                "{\"account\":{\"id\":1,\"name\":\"Alpha\"},\"items\":[]}",
            )
        },
    );

    let run = hosts.run(
        &["accounts", "switch", "1"],
        &[("MVDM_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["account"]["name"], "Alpha");
    let request = &hosts.api_requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, format!("{ACCOUNTS_PATH}/1/switch"));
    assert!(request.body.is_empty());
    assert_eq!(request.header("Authorization"), Some("Bearer cmp_pat_test"));
}

#[test]
fn accounts_switch_needs_a_numeric_id() {
    let hosts = Hosts::start(|_| Reply::empty(404), ok_api);

    let run = hosts.run(
        &["accounts", "switch", "alpha"],
        &[("MVDM_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 2, "{run:#?}");
    assert!(hosts.compliance.requests().is_empty());
}

/// A Product that serves `incomplete` as its description while `complete` is false, and the description with the
/// Account operations moved once it is true.
fn product_with_late_accounts(
    incomplete: Value,
    complete: Arc<AtomicBool>,
) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    let incomplete = incomplete.to_string();
    let complete_description = with_description(accounts_api);
    move |request| {
        if request.path() == DESCRIPTION_PATH && !complete.load(Ordering::SeqCst) {
            Reply::json(200, &incomplete)
        } else {
            complete_description(request)
        }
    }
}

#[test]
fn a_description_without_accounts_list_fails_status_and_is_refetched_once_to_find_it() {
    let mut incomplete = description_with_accounts_moved();
    let paths = incomplete["paths"].as_object_mut().expect("paths");
    paths.remove(ACCOUNTS_PATH).expect("accounts.list");
    let complete = Arc::new(AtomicBool::new(false));
    let hosts = Hosts::start_serving(
        tempfile::tempdir().expect("a config folder"),
        |_| Reply::empty(404),
        product_with_late_accounts(incomplete, Arc::clone(&complete)),
    );
    let token = [("MVDM_TOKEN", "cmp_pat_test")];

    // A fresh fetch lacks the operation: nothing newer to fetch.
    let missing = hosts.run(&["status"], &token);

    assert_eq!(missing.code, 1, "{missing:#?}");
    assert_eq!(missing.stdout, "");
    let error = missing.stderr_json();
    assert_eq!(error["error"], "discovery");
    let message = error["message"].as_str().expect("a message");
    assert!(message.contains("compliance"), "{message}");
    assert!(message.contains("accounts.list"), "{message}");
    assert_eq!(hosts.compliance.description_fetches(), 1);
    assert!(hosts.api_requests().is_empty());

    // The cached copy lacks it too: one refetch finds it where the Product now places it.
    complete.store(true, Ordering::SeqCst);
    let found = hosts.run(&["status"], &token);

    assert_eq!(found.code, 0, "{found:#?}");
    assert_eq!(found.stdout_json()["account"]["id"], 2);
    assert_eq!(hosts.compliance.description_fetches(), 2);
}

#[test]
fn a_switch_operation_without_one_path_parameter_is_a_discovery_failure() {
    let mut document = description_with_accounts_moved();
    document["paths"][format!("{ACCOUNTS_PATH}/{{accountId}}/switch")]["post"]["parameters"] =
        json!([]);
    let description = document.to_string();
    let hosts = Hosts::start_serving(
        tempfile::tempdir().expect("a config folder"),
        |_| Reply::empty(404),
        move |request| {
            if request.path() == DESCRIPTION_PATH {
                Reply::json(200, &description)
            } else {
                Reply::empty(404)
            }
        },
    );

    let run = hosts.run(
        &["accounts", "switch", "1"],
        &[("MVDM_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 1, "{run:#?}");
    let error = run.stderr_json();
    assert_eq!(error["error"], "discovery");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("accounts.switch"),
        "{error}"
    );
    assert!(hosts.api_requests().is_empty());
}

#[test]
fn two_account_commands_fetch_the_description_once() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);
    let token = [("MVDM_TOKEN", "cmp_pat_test")];

    assert_eq!(hosts.run(&["accounts", "list"], &token).code, 0);
    assert_eq!(hosts.run(&["status"], &token).code, 0);

    assert_eq!(hosts.compliance.description_fetches(), 1);
    assert_eq!(hosts.api_requests().len(), 2);
}

/// Starts `mvdm login` with a launcher that claims to have opened the browser, and returns it with the
/// `browser_opened` line it wrote first.
#[cfg(target_os = "linux")]
fn start_browser_login(hosts: &Hosts, launcher: &std::path::Path) -> (std::process::Child, Value) {
    use std::io::{BufRead, BufReader};
    use std::os::unix::fs::PermissionsExt;
    use std::process::Stdio;

    let xdg_open = launcher.join("xdg-open");
    std::fs::write(&xdg_open, "#!/bin/sh\nexit 0\n").expect("write the launcher");
    std::fs::set_permissions(&xdg_open, std::fs::Permissions::from_mode(0o755))
        .expect("make the launcher runnable");
    let path = format!(
        "{}:{}",
        launcher.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let auth = hosts.auth.url();
    let config = hosts.config.path().to_string_lossy().into_owned();
    let mut child = support::command(
        std::path::Path::new("."),
        &["login"],
        &[
            ("MVDM_AUTH_URL", auth.as_str()),
            ("MVDM_CONFIG_DIR", config.as_str()),
            ("DISPLAY", ":99"),
            ("PATH", path.as_str()),
        ],
        hosts.config.path(),
    )
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("start mvdm login");
    let mut line = String::new();
    BufReader::new(child.stderr.take().expect("stderr"))
        .read_line(&mut line)
        .expect("read stderr");
    let opened = serde_json::from_str(&line).unwrap_or_else(|error| panic!("{error}: {line}"));
    (child, opened)
}

#[cfg(target_os = "linux")]
fn browser() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .new_agent()
}

#[cfg(target_os = "linux")]
#[test]
fn login_signs_in_through_the_browser_with_pkce_and_a_loopback_redirect() {
    use base64::Engine;

    let hosts = Hosts::start(
        |request| match request.path() {
            "/connect/token" => signed_in_tokens(),
            _ => Reply::empty(404),
        },
        accounts_api,
    );
    let launcher = tempfile::tempdir().expect("a launcher folder");

    let (child, opened) = start_browser_login(&hosts, launcher.path());

    assert_eq!(opened["status"], "browser_opened");
    let authorize = opened["authorizeUrl"].as_str().expect("an authorize URL");
    let (endpoint, query) = authorize.split_once('?').expect("a query");
    assert_eq!(endpoint, format!("{}/connect/authorize", hosts.auth.url()));
    let parameter = |name: &str| support::form_value(query, name);
    assert_eq!(parameter("response_type").as_deref(), Some("code"));
    assert_eq!(parameter("client_id").as_deref(), Some(CLIENT_ID));
    assert_eq!(parameter("scope").as_deref(), Some("openid"));
    assert_eq!(parameter("resource"), Some(hosts.resource()));
    assert_eq!(parameter("code_challenge_method").as_deref(), Some("S256"));
    let redirect = parameter("redirect_uri").expect("a redirect");
    assert!(
        redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/callback"),
        "{redirect}"
    );
    let state = parameter("state").expect("a state");

    let browser = browser();
    let stray = browser
        .get(redirect.replace("/callback", "/favicon.ico"))
        .call()
        .expect("the loopback answers");
    assert_eq!(stray.status(), 404);
    let mut page = browser
        .get(format!("{redirect}?code=code-1&state={state}"))
        .call()
        .expect("the loopback answers");
    assert_eq!(page.status(), 200);
    assert!(
        page.body_mut()
            .read_to_string()
            .unwrap()
            .contains("You can close this window")
    );

    let output = child.wait_with_output().expect("login finishes");
    assert_eq!(output.status.code(), Some(0));
    let printed: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(printed["status"], "logged_in");
    assert_eq!(printed["user"]["name"], "Ada Lovelace");

    let redeem = hosts.auth_requests("/connect/token");
    assert_eq!(redeem.len(), 1);
    assert_eq!(
        redeem[0].form("grant_type").as_deref(),
        Some("authorization_code")
    );
    assert_eq!(redeem[0].form("code").as_deref(), Some("code-1"));
    assert_eq!(redeem[0].form("redirect_uri"), Some(redirect));
    assert_eq!(redeem[0].form("client_id").as_deref(), Some(CLIENT_ID));
    let verifier = redeem[0].form("code_verifier").expect("a verifier");
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ));
    assert_eq!(parameter("code_challenge"), Some(challenge));
    assert_eq!(hosts.stored()["accessToken"], "access-1");
}

#[cfg(target_os = "linux")]
#[test]
fn a_browser_answer_with_the_wrong_state_is_refused_and_the_login_keeps_waiting() {
    let hosts = Hosts::start(|_| signed_in_tokens(), accounts_api);
    let launcher = tempfile::tempdir().expect("a launcher folder");

    let (child, opened) = start_browser_login(&hosts, launcher.path());
    let query = opened["authorizeUrl"]
        .as_str()
        .unwrap()
        .split_once('?')
        .unwrap()
        .1;
    let redirect = support::form_value(query, "redirect_uri").unwrap();
    let state = support::form_value(query, "state").unwrap();
    let forged = browser()
        .get(format!("{redirect}?code=forged-code&state=forged"))
        .call()
        .expect("the loopback answers");
    assert_eq!(forged.status(), 400);
    let genuine = browser()
        .get(format!("{redirect}?code=code-1&state={state}"))
        .call()
        .expect("the loopback answers");
    assert_eq!(genuine.status(), 200);

    let output = child.wait_with_output().expect("login finishes");
    assert_eq!(output.status.code(), Some(0));
    let redeem = hosts.auth_requests("/connect/token");
    assert_eq!(redeem.len(), 1);
    assert_eq!(redeem[0].form("code").as_deref(), Some("code-1"));
}

#[cfg(target_os = "linux")]
#[test]
fn a_browser_answer_with_an_error_ends_the_login_and_stores_nothing() {
    let hosts = Hosts::start(|_| signed_in_tokens(), accounts_api);
    let launcher = tempfile::tempdir().expect("a launcher folder");

    let (child, opened) = start_browser_login(&hosts, launcher.path());
    let query = opened["authorizeUrl"]
        .as_str()
        .unwrap()
        .split_once('?')
        .unwrap()
        .1;
    let redirect = support::form_value(query, "redirect_uri").unwrap();
    let state = support::form_value(query, "state").unwrap();
    let page = browser()
        .get(format!("{redirect}?error=access_denied&state={state}"))
        .call()
        .expect("the loopback answers");

    assert_eq!(page.status(), 400);
    let output = child.wait_with_output().expect("login finishes");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(hosts.auth_requests("/connect/token").is_empty());
    assert!(!credentials_path(hosts.config.path()).exists());
}

#[test]
fn an_invalid_grant_after_another_command_rotated_the_token_keeps_the_newer_sign_in() {
    let config = tempfile::tempdir().expect("a config folder");
    let folder = config.path().to_path_buf();
    // Another command refreshed first: by the time Auth refuses this command's token, the file holds its successor.
    let auth = move |request: &Recorded| {
        store_sign_in(
            &folder,
            &request.origin(),
            "access-2",
            unix_now() + 3600,
            "refresh-2",
        );
        oauth_error("invalid_grant")
    };
    let hosts = Hosts::start_in(config, auth, |request| {
        if request.header("Authorization") == Some("Bearer access-2") {
            Reply::json(200, "{\"ok\":1}")
        } else {
            Reply::problem(401, "{\"type\":\"unauthorized\"}")
        }
    });
    hosts.store("access-1", unix_now(), "refresh-1");

    let run = hosts.run(&["compliance", "api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "ok": 1 }));
    assert_eq!(hosts.stored()["refreshToken"], "refresh-2");
}
