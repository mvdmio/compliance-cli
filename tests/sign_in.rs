mod support;

use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Value, json};
use support::{
    FakeServer, Recorded, Reply, Run, accounts_list, auth_metadata, compliance, credentials_path,
    id_token, oauth_error, read_credentials, resource_metadata, store_sign_in, tokens, unix_now,
};
use tempfile::TempDir;

const CLIENT_ID: &str = "mvdmio.compliance_cli";

/// A fake Compliance host whose Protected Resource Metadata names a separate fake Auth, and a config folder.
struct Hosts {
    auth: FakeServer,
    compliance: FakeServer,
    config: TempDir,
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
        let auth = FakeServer::start(move |request| {
            auth_metadata(request).unwrap_or_else(|| auth(request))
        });
        let issuer = format!("{}/", auth.url());
        let compliance = FakeServer::start(move |request| {
            resource_metadata(request, &issuer).unwrap_or_else(|| api(request))
        });
        Hosts {
            auth,
            compliance,
            config,
        }
    }

    fn host(&self) -> String {
        self.compliance.url()
    }

    fn resource(&self) -> String {
        format!("{}/api", self.host())
    }

    fn run(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let host = self.host();
        let config = self.config.path().to_string_lossy().into_owned();
        let mut all = vec![
            ("COMPLIANCE_URL", host.as_str()),
            ("COMPLIANCE_CONFIG_DIR", config.as_str()),
        ];
        all.extend_from_slice(env);
        compliance(args, &all)
    }

    fn store(&self, access_token: &str, expires_at: u64, refresh_token: &str) {
        store_sign_in(
            self.config.path(),
            &self.host(),
            access_token,
            expires_at,
            refresh_token,
        );
    }

    fn stored(&self) -> Value {
        read_credentials(self.config.path())["hosts"][self.host()].clone()
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
        "/api/v1/accounts" => accounts_list(),
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
            "host": hosts.host(),
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
fn auth_is_found_only_through_the_protected_resource_metadata() {
    let hosts = Hosts::start(device_auth(signed_in_tokens), accounts_api);

    let run = hosts.run(&["login", "--device"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    let compliance_paths: Vec<String> = hosts
        .compliance
        .requests()
        .iter()
        .map(|request| request.path().to_string())
        .collect();
    assert_eq!(
        compliance_paths,
        vec![
            "/.well-known/oauth-protected-resource/api",
            "/api/v1/accounts"
        ]
    );
    assert_eq!(
        hosts.auth.requests()[0].path(),
        "/.well-known/openid-configuration"
    );
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

    let call = hosts.run(&["api", "GET", "/api/v1/accounts"], &[]);
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

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

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
fn a_fresh_access_token_is_used_without_a_refresh() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert!(hosts.auth.requests().is_empty());
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

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

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

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stderr_json()["type"], "unauthorized");
    assert_eq!(hosts.auth_requests("/connect/token").len(), 1);
    assert_eq!(hosts.api_requests().len(), 2);
}

#[test]
fn a_refresh_answered_invalid_grant_removes_only_that_hosts_entry_and_asks_for_login() {
    let hosts = Hosts::start(|_| oauth_error("invalid_grant"), ok_api);
    hosts.store("access-1", unix_now(), "refresh-1");
    store_sign_in(
        hosts.config.path(),
        "https://other.example.test",
        "other-access",
        unix_now() + 3600,
        "other-refresh",
    );

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 1, "{run:#?}");
    let error = run.stderr_json();
    assert_eq!(error["error"], "not-signed-in");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("compliance login"),
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
fn compliance_token_wins_over_the_stored_sign_in_and_is_never_refreshed_or_stored() {
    let hosts = Hosts::start(rotating_auth, ok_api);
    hosts.store("access-1", unix_now(), "refresh-1");

    let run = hosts.run(
        &["api", "GET", "/api/v1/risks"],
        &[("COMPLIANCE_TOKEN", "cmp_pat_test")],
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
fn logout_revokes_the_refresh_token_then_deletes_the_file() {
    let hosts = Hosts::start(
        |request| match request.path() {
            "/connect/revoke" => Reply::empty(200),
            _ => Reply::empty(404),
        },
        ok_api,
    );
    hosts.store("access-1", unix_now() + 3600, "refresh-1");

    let run = hosts.run(&["logout"], &[("COMPLIANCE_TOKEN", "cmp_pat_test")]);

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

    let run = hosts.run(&["status"], &[("COMPLIANCE_TOKEN", "cmp_pat_test")]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(
        run.stdout_json(),
        json!({
            "host": hosts.host(),
            "signedIn": true,
            "credential": { "kind": "personal-token", "source": "COMPLIANCE_TOKEN" },
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
            "host": hosts.host(),
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

    let run = hosts.run(&["status"], &[("COMPLIANCE_TOKEN", "cmp_pat_revoked")]);

    assert_eq!(run.code, 1, "{run:#?}");
    assert_eq!(run.stdout, "");
    assert_eq!(run.stderr.trim_end(), problem);
}

#[test]
fn accounts_list_gets_the_accounts_and_prints_them() {
    let hosts = Hosts::start(|_| Reply::empty(404), accounts_api);

    let run = hosts.run(
        &["accounts", "list"],
        &[("COMPLIANCE_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["items"][1]["name"], "Beta");
    let request = &hosts.api_requests()[0];
    assert_eq!(request.method, "GET");
    assert_eq!(request.url, "/api/v1/accounts");
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
        &[("COMPLIANCE_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json()["account"]["name"], "Alpha");
    let request = &hosts.api_requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "/api/v1/accounts/1/switch");
    assert!(request.body.is_empty());
}

#[test]
fn accounts_switch_needs_a_numeric_id() {
    let hosts = Hosts::start(|_| Reply::empty(404), ok_api);

    let run = hosts.run(
        &["accounts", "switch", "alpha"],
        &[("COMPLIANCE_TOKEN", "cmp_pat_test")],
    );

    assert_eq!(run.code, 2, "{run:#?}");
    assert!(hosts.compliance.requests().is_empty());
}

/// Starts `compliance login` with a launcher that claims to have opened the browser, and returns it with the
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
    let host = hosts.host();
    let config = hosts.config.path().to_string_lossy().into_owned();
    let mut child = support::command(
        std::path::Path::new("."),
        &["login"],
        &[
            ("COMPLIANCE_URL", host.as_str()),
            ("COMPLIANCE_CONFIG_DIR", config.as_str()),
            ("DISPLAY", ":99"),
            ("PATH", path.as_str()),
        ],
        hosts.config.path(),
    )
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("start compliance login");
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
        let resource = request.form("resource").expect("a resource");
        let host = resource.strip_suffix("/api").expect("the API resource");
        store_sign_in(&folder, host, "access-2", unix_now() + 3600, "refresh-2");
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

    let run = hosts.run(&["api", "GET", "/api/v1/risks"], &[]);

    assert_eq!(run.code, 0, "{run:#?}");
    assert_eq!(run.stdout_json(), json!({ "ok": 1 }));
    assert_eq!(hosts.stored()["refreshToken"], "refresh-2");
}
