use serde_json::json;

use crate::config;
use crate::discovery;
use crate::failure::Failure;
use crate::http;
use crate::oauth;
use crate::output;
use crate::store;

/// `mvdm logout`: revokes the stored refresh token at Auth, which ends the Agent connection, then forgets
/// the sign-in. The entry stays when Auth cannot be told, so the User can retry. `MVDM_TOKEN` is not touched.
pub fn run() -> Result<(), Failure> {
    let auth = config::auth();
    if let Some(sign_in) = store::load(&auth)? {
        let agent = http::agent();
        let endpoint = discovery::revocation_endpoint(&agent, &auth)?;
        oauth::revoke(&agent, &endpoint, &sign_in)?;
        store::remove(&auth)?;
    }
    output::print_json(&json!({ "status": "logged_out" }));
    Ok(())
}
