use std::env;

use crate::failure::Failure;

const DEFAULT_HOST: &str = "https://compliance.mvdm.io";

/// The Compliance host: `COMPLIANCE_URL` when set, without a trailing slash.
pub fn host() -> String {
    match env::var("COMPLIANCE_URL") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}

/// What a request authenticates with.
#[derive(Clone, Debug)]
pub enum Credential {
    /// A Personal token from `COMPLIANCE_TOKEN`.
    PersonalToken(String),
}

impl Credential {
    pub fn bearer(&self) -> &str {
        match self {
            Credential::PersonalToken(token) => token,
        }
    }
}

/// The credential in use, or `not-signed-in` when there is none.
pub fn credential() -> Result<Credential, Failure> {
    match env::var("COMPLIANCE_TOKEN") {
        Ok(token) if !token.trim().is_empty() => {
            Ok(Credential::PersonalToken(token.trim().to_string()))
        }
        _ => Err(Failure::local(
            "not-signed-in",
            "No credential. Run `compliance login`, or set COMPLIANCE_TOKEN to a Personal token.",
        )),
    }
}
