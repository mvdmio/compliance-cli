use std::env;

use ureq::Agent;

use crate::discovery;
use crate::failure::{ErrorCode, Failure};
use crate::oauth;
use crate::store::{self, SignIn};

/// What a request authenticates with.
#[derive(Clone, Debug)]
pub enum Credential {
    /// A Personal token from `MVDM_TOKEN`. Never refreshed or stored.
    PersonalToken(String),
    /// The stored sign-in: an Agent connection.
    AgentConnection(SignIn),
}

impl Credential {
    pub fn bearer(&self) -> &str {
        match self {
            Credential::PersonalToken(token) => token,
            Credential::AgentConnection(sign_in) => &sign_in.access_token,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Credential::PersonalToken(_) => "personal-token",
            Credential::AgentConnection(_) => "agent-connection",
        }
    }

    pub fn source(&self) -> &'static str {
        match self {
            Credential::PersonalToken(_) => "MVDM_TOKEN",
            Credential::AgentConnection(_) => "stored sign-in",
        }
    }
}

/// `MVDM_TOKEN` when set, else the stored sign-in for the Auth address.
pub fn find(auth: &str) -> Result<Option<Credential>, Failure> {
    if let Ok(token) = env::var("MVDM_TOKEN")
        && !token.trim().is_empty()
    {
        return Ok(Some(Credential::PersonalToken(token.trim().to_string())));
    }
    Ok(store::load(auth)?.map(Credential::AgentConnection))
}

/// The credential in use, or `not-signed-in` when there is none.
pub fn require(auth: &str) -> Result<Credential, Failure> {
    find(auth)?.ok_or_else(|| {
        Failure::local(
            ErrorCode::NotSignedIn,
            "No credential. Run `mvdm login`, or set MVDM_TOKEN to a Personal token.",
        )
    })
}

/// Trades the refresh token for new tokens and stores them at once, because Auth rotates the refresh token. A
/// refresh token Auth no longer accepts removes the Auth address's entry.
pub fn refresh(agent: &Agent, auth: &str, sign_in: &SignIn) -> Result<SignIn, Failure> {
    let Some(refresh_token) = sign_in.refresh_token.as_deref() else {
        return Err(sign_in_ended(auth));
    };
    let server = discovery::discover_for_refresh(agent, auth)?;
    let form = server.form(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ]);
    match oauth::request_tokens(agent, &server, &form, Some(sign_in))? {
        Ok(refreshed) => {
            store::save(auth, &refreshed)?;
            Ok(refreshed)
        }
        // Another command may have used this refresh token first and stored its successor.
        Err(error) if error.error == "invalid_grant" => {
            match store::remove_ended(auth, refresh_token)? {
                Some(newer) => Ok(newer),
                None => Err(sign_in_ended(auth)),
            }
        }
        Err(error) => Err(error.into_failure()),
    }
}

fn sign_in_ended(auth: &str) -> Failure {
    Failure::local(
        ErrorCode::NotSignedIn,
        format!("The sign-in for {auth} has ended. Run `mvdm login` to sign in again."),
    )
}
