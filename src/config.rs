use std::env;

const DEFAULT_HOST: &str = "https://compliance.mvdm.io";

/// The Compliance host: `COMPLIANCE_URL` when set, without a trailing slash.
pub fn host() -> String {
    match env::var("COMPLIANCE_URL") {
        Ok(url) if !url.trim().is_empty() => url.trim().trim_end_matches('/').to_string(),
        _ => DEFAULT_HOST.to_string(),
    }
}
