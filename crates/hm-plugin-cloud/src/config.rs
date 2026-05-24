//! Runtime configuration. API base URL and any other knobs.

use std::collections::BTreeMap;

pub(crate) const DEFAULT_API_BASE: &str = "https://api.harmont.dev";

pub(crate) struct Config {
    pub api_base: String,
}

impl Config {
    pub(crate) fn from_env(env: &BTreeMap<String, String>) -> Self {
        let api_base = env
            .get("HARMONT_API_URL")
            .cloned()
            .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
        Self { api_base }
    }
}
