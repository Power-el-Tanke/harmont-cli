//! Runtime configuration. API base URL and any other knobs the
//! plugin reads at start-of-call.

use std::collections::BTreeMap;

#[allow(
    dead_code,
    reason = "consumed by `Config::from_env` once verbs land in a later cluster"
)]
pub(crate) const DEFAULT_API_BASE: &str = "https://api.harmont.dev";

pub(crate) struct Config {
    pub api_base: String,
}

impl Config {
    #[allow(dead_code, reason = "consumed by verbs in a later cluster")]
    pub(crate) fn from_env(env: &BTreeMap<String, String>) -> Self {
        let api_base = env
            .get("HARMONT_API_URL")
            .cloned()
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
        Self { api_base }
    }
}
