use serde_json::json;

use crate::config;
use crate::discovery;
use crate::failure::Failure;
use crate::http;
use crate::oauth;
use crate::output;
use crate::store;

pub fn run() -> Result<(), Failure> {
    let host = config::host();
    if let Some(sign_in) = store::load(&host)? {
        let agent = http::agent();
        let server = discovery::discover(&agent, &host)?;
        oauth::revoke(&agent, &server, &sign_in)?;
        store::remove(&host)?;
    }
    output::print_json(&json!({ "status": "logged_out" }));
    Ok(())
}
