use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use percent_encoding::{percent_decode_str, utf8_percent_encode};
use serde::Deserialize;
use serde_json::json;
use ureq::Agent;

use crate::accounts;
use crate::cli::LoginArgs;
use crate::config;
use crate::credential::Credential;
use crate::discovery::{self, AuthServer};
use crate::failure::{ErrorCode, Failure};
use crate::http::{self, Client};
use crate::oauth::{self, SCOPE};
use crate::output;
use crate::request::UNRESERVED;
use crate::store::{self, SignIn};

const BROWSER_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const ACCEPT_POLL: Duration = Duration::from_millis(100);
const DEFAULT_DEVICE_INTERVAL: u64 = 5;
/// RFC 8628 §3.5: `slow_down` adds 5 seconds to the polling interval.
const SLOW_DOWN_STEP: u64 = 5;
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// `compliance login`: signs in through Auth, in the browser or with a device code, and stores the sign-in.
pub fn run(args: LoginArgs) -> Result<(), Failure> {
    let host = config::host();
    let agent = http::agent();
    let server = discovery::discover(&agent, &host)?;

    let browser = if args.device {
        None
    } else {
        browser_sign_in(&agent, &server)?
    };
    let sign_in = match browser {
        Some(sign_in) => sign_in,
        None => device_sign_in(&agent, &server)?,
    };
    store::save(&host, &sign_in)?;

    let user = sign_in.user.clone();
    let mut client = Client::new(host.clone(), Credential::AgentConnection(sign_in));
    let account = accounts::current(&mut client)?;
    output::print_json(&json!({
        "status": "logged_in",
        "host": host,
        "user": user,
        "account": account,
    }));
    Ok(())
}

/// The authorization code flow with PKCE through a loopback redirect (RFC 8252). `None` when no browser opens.
fn browser_sign_in(agent: &Agent, server: &AuthServer) -> Result<Option<SignIn>, Failure> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| {
        Failure::local(
            ErrorCode::Login,
            format!("Cannot listen on 127.0.0.1: {error}"),
        )
    })?;
    let port = listener
        .local_addr()
        .map_err(|error| Failure::local(ErrorCode::Login, error.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let verifier = oauth::random_text(32);
    let state = oauth::random_text(16);
    let challenge = oauth::challenge_of(&verifier);
    let authorize_url = with_query(
        &server.authorization_endpoint,
        &server.form(&[
            ("response_type", "code"),
            ("redirect_uri", &redirect_uri),
            ("scope", SCOPE),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]),
    );

    if !can_open_browser() || open::that(&authorize_url).is_err() {
        return Ok(None);
    }
    output::print_stderr_json(
        &json!({ "status": "browser_opened", "authorizeUrl": authorize_url }),
    );

    let code = wait_for_code(&listener, &state)?;
    let form = server.form(&[
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier.as_str()),
    ]);
    oauth::request_tokens(agent, server, &form, None)?
        .map(Some)
        .map_err(oauth::OAuthError::into_failure)
}

/// A desktop with no display cannot show a browser, such as a Linux server reached over SSH.
fn can_open_browser() -> bool {
    if cfg!(any(target_os = "windows", target_os = "macos")) {
        return true;
    }
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String, Failure> {
    listener
        .set_nonblocking(true)
        .map_err(|error| Failure::local(ErrorCode::Login, error.to_string()))?;
    let deadline = Instant::now() + BROWSER_TIMEOUT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = answer_callback(stream, state) {
                    return result;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(ACCEPT_POLL),
            Err(error) => return Err(Failure::local(ErrorCode::Login, error.to_string())),
        }
    }
    Err(Failure::local(
        ErrorCode::LoginTimeout,
        "No sign-in arrived within 5 minutes. Run `compliance login` again.",
    ))
}

/// Answers one browser request. `None` keeps waiting: for a request that is not the callback, such as a favicon,
/// and for a callback without this login's `state`, which did not come from this login's authorize request.
fn answer_callback(stream: TcpStream, state: &str) -> Option<Result<String, Failure>> {
    // Ignored: a stream that will not block or time out only risks this one request.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut request_line = String::new();
    if BufReader::new(&stream)
        .read_line(&mut request_line)
        .is_err()
    {
        return None;
    }
    let target = request_line.split_whitespace().nth(1).unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path != "/callback" {
        respond(&stream, "404 Not Found", "Not found.");
        return None;
    }

    let parameter = |name: &str| query_value(query, name);
    if parameter("state").as_deref() != Some(state) {
        respond(
            &stream,
            "400 Bad Request",
            "This answer does not belong to the waiting sign-in.",
        );
        return None;
    }
    let result = if let Some(error) = parameter("error") {
        Err(oauth::OAuthError {
            error,
            description: parameter("error_description"),
        }
        .into_failure())
    } else {
        parameter("code")
            .ok_or_else(|| Failure::local(ErrorCode::Login, "The sign-in answer carried no code."))
    };
    match &result {
        Ok(_) => respond(
            &stream,
            "200 OK",
            "You are signed in to the Compliance CLI. You can close this window.",
        ),
        Err(_) => respond(
            &stream,
            "400 Bad Request",
            "The sign-in did not finish. See the terminal. You can close this window.",
        ),
    }
    Some(result)
}

fn respond(mut stream: &TcpStream, status: &str, text: &str) {
    let page = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Compliance CLI</title></head><body><p>{text}</p></body></html>"
    );
    // Ignored: the browser may have gone; the terminal carries the outcome.
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.flush();
}

fn with_query(url: &str, parameters: &[(&str, &str)]) -> String {
    let query = parameters
        .iter()
        .map(|(name, value)| format!("{name}={}", utf8_percent_encode(value, UNRESERVED)))
        .collect::<Vec<_>>()
        .join("&");
    let separator = if url.contains('?') { '&' } else { '?' };
    format!("{url}{separator}{query}")
}

fn query_value(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name).then(|| {
            percent_decode_str(&value.replace('+', " "))
                .decode_utf8_lossy()
                .into_owned()
        })
    })
}

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

/// The device flow (RFC 8628): shows the code and link on stderr, then polls until the User has answered.
fn device_sign_in(agent: &Agent, server: &AuthServer) -> Result<SignIn, Failure> {
    let form = server.form(&[("scope", SCOPE)]);
    let body = oauth::post_form(agent, &server.device_authorization_endpoint, &form)?
        .map_err(oauth::OAuthError::into_failure)?;
    let device: DeviceAuthorization = serde_json::from_value(body).map_err(|error| {
        Failure::local(
            ErrorCode::Auth,
            format!("The device endpoint's answer is not a device authorization: {error}"),
        )
    })?;
    output::print_stderr_json(&json!({
        "status": "device_code",
        "verificationUriComplete": device.verification_uri_complete,
        "verificationUri": device.verification_uri,
        "userCode": device.user_code,
        "expiresIn": device.expires_in,
    }));

    let form = server.form(&[
        ("grant_type", DEVICE_CODE_GRANT),
        ("device_code", device.device_code.as_str()),
    ]);
    let deadline = Instant::now() + Duration::from_secs(device.expires_in);
    let mut interval = device.interval.unwrap_or(DEFAULT_DEVICE_INTERVAL);
    loop {
        thread::sleep(Duration::from_secs(interval));
        if Instant::now() >= deadline {
            return Err(Failure::local(
                ErrorCode::ExpiredToken,
                "The device code expired before the sign-in finished. Run `compliance login` again.",
            ));
        }
        match oauth::request_tokens(agent, server, &form, None)? {
            Ok(sign_in) => return Ok(sign_in),
            Err(error) if error.error == "authorization_pending" => {}
            Err(error) if error.error == "slow_down" => interval += SLOW_DOWN_STEP,
            Err(error) => return Err(error.into_failure()),
        }
    }
}
