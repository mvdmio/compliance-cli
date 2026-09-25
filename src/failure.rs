use std::io;
use std::path::Path;
use std::process::ExitCode;

use serde_json::json;

use crate::output;

/// Every way a command ends other than success, and the exit code each maps to.
#[derive(Debug)]
pub enum Failure {
    /// A mistake in the command line: exit 2.
    Usage(String),
    /// A failure before or outside the API, as `{"error":<code>,"message":…}` on stderr: exit 1.
    Local { code: &'static str, message: String },
    /// The API's ProblemDetails body as it came: exit 1.
    Problem(Vec<u8>),
    /// The API's 403 `browser-handoff`: a person must finish the act at `handoff_url`. Exit 3.
    BrowserHandoff {
        reason: Option<String>,
        handoff_url: Option<String>,
    },
}

impl Failure {
    pub fn local(code: &'static str, message: impl Into<String>) -> Self {
        Failure::Local {
            code,
            message: message.into(),
        }
    }

    /// The local error `file`, for a file the CLI could not read or write.
    pub fn file(path: &Path, error: io::Error) -> Self {
        Failure::local("file", format!("{}: {error}", path.display()))
    }

    pub fn report(self) -> ExitCode {
        match self {
            Failure::Usage(message) => {
                output::print_stderr_json(&json!({ "error": "usage", "message": message }));
                ExitCode::from(2)
            }
            Failure::Local { code, message } => {
                output::print_stderr_json(&json!({ "error": code, "message": message }));
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
