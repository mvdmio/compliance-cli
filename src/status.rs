use serde_json::{Value, json};

use crate::accounts;
use crate::config;
use crate::credential::{self, Credential};
use crate::failure::Failure;
use crate::http::Client;
use crate::output;

/// `compliance status`: the host, the credential in use, who it acts as, and in which Account.
pub fn run() -> Result<(), Failure> {
    let host = config::host();
    let Some(credential) = credential::find(&host)? else {
        output::print_json(&json!({
            "host": host,
            "signedIn": false,
            "credential": null,
            "user": null,
            "account": null,
        }));
        return Ok(());
    };

    let mut client = Client::new(host.clone(), credential);
    let account = accounts::current(&mut client)?;
    let credential = client.credential();
    // A Personal token carries no `id_token`, so only a stored sign-in knows the User.
    let user = match credential {
        Credential::AgentConnection(sign_in) => json!(sign_in.user),
        Credential::PersonalToken(_) => Value::Null,
    };
    output::print_json(&json!({
        "host": host,
        "signedIn": true,
        "credential": { "kind": credential.kind(), "source": credential.source() },
        "user": user,
        "account": account,
    }));
    Ok(())
}
