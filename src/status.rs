use serde_json::{Value, json};

use crate::accounts;
use crate::config;
use crate::credential::{self, Credential};
use crate::failure::Failure;
use crate::http::Client;
use crate::output;
use crate::products;

/// `mvdm status`: the Auth address, the credential in use, who it acts as, and in which Account.
pub fn run() -> Result<(), Failure> {
    let auth = config::auth();
    let Some(credential) = credential::find(&auth)? else {
        output::print_json(&json!({
            "auth": auth,
            "signedIn": false,
            "credential": null,
            "user": null,
            "account": null,
        }));
        return Ok(());
    };

    let product = products::account_product()?;
    let mut client = Client::new(&product, credential);
    let account = accounts::current(&product, &mut client)?;
    let credential = client.credential();
    // A Personal token carries no `id_token`, so only a stored sign-in knows the User.
    let user = match credential {
        Credential::AgentConnection(sign_in) => json!(sign_in.user),
        Credential::PersonalToken(_) => Value::Null,
    };
    output::print_json(&json!({
        "auth": auth,
        "signedIn": true,
        "credential": { "kind": credential.kind(), "source": credential.source() },
        "user": user,
        "account": account,
    }));
    Ok(())
}
