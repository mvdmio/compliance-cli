use std::io;
use std::path::Path;
use std::process::ExitCode;

use serde_json::json;

use crate::output;

#[derive(Clone, Copy, Debug)]
pub enum ErrorCode {
    AccessDenied,
    Auth,
    BinaryResponse,
    Config,
    Credentials,
    Discovery,
    ExpiredToken,
    File,
    Http,
    InvalidResponse,
    Login,
    LoginTimeout,
    Network,
    NotSignedIn,
    Upload,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::AccessDenied => "access-denied",
            ErrorCode::Auth => "auth",
            ErrorCode::BinaryResponse => "binary-response",
            ErrorCode::Config => "config",
            ErrorCode::Credentials => "credentials",
            ErrorCode::Discovery => "discovery",
            ErrorCode::ExpiredToken => "expired-token",
            ErrorCode::File => "file",
            ErrorCode::Http => "http",
            ErrorCode::InvalidResponse => "invalid-response",
            ErrorCode::Login => "login",
            ErrorCode::LoginTimeout => "login-timeout",
            ErrorCode::Network => "network",
            ErrorCode::NotSignedIn => "not-signed-in",
            ErrorCode::Upload => "upload",
        }
    }
}

#[derive(Debug)]
pub enum Failure {
    Usage(String),
    Local {
        code: ErrorCode,
        message: String,
    },
    Problem(Vec<u8>),
    BrowserHandoff {
        reason: Option<String>,
        handoff_url: Option<String>,
    },
}

impl Failure {
    pub fn local(code: ErrorCode, message: impl Into<String>) -> Self {
        Failure::Local {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_response(message: impl Into<String>) -> Self {
        Failure::local(ErrorCode::InvalidResponse, message)
    }

    pub fn file(path: &Path, error: io::Error) -> Self {
        Failure::local(ErrorCode::File, format!("{}: {error}", path.display()))
    }

    pub fn report(self) -> ExitCode {
        match self {
            Failure::Usage(message) => {
                output::print_stderr_json(&json!({ "error": "usage", "message": message }));
                ExitCode::from(2)
            }
            Failure::Local { code, message } => {
                output::print_stderr_json(&json!({ "error": code.as_str(), "message": message }));
                ExitCode::from(1)
            }
            Failure::Problem(body) => {
                output::print_stderr_raw(&body);
                ExitCode::from(1)
            }
            Failure::BrowserHandoff {
                reason,
                handoff_url,
            } => {
                output::print_json(&json!({
                    "status": "browser_handoff",
                    "reason": reason,
                    "handoffUrl": handoff_url,
                }));
                ExitCode::from(3)
            }
        }
    }
}
