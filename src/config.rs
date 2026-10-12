use std::env;

const DEFAULT_AUTH: &str = "https://auth.mvdm.io";

/// Auth's address: `MVDM_AUTH_URL` when set, without a trailing slash.
pub fn auth() -> String {
    match env::var("MVDM_AUTH_URL") {
        Ok(url) if !url.trim().is_empty() => trim_address(&url),
        _ => DEFAULT_AUTH.to_string(),
    }
}

/// An address as typed, without surrounding spaces or a trailing slash.
pub fn trim_address(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}
