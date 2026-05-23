//! On-disk credential storage via direct file I/O.
//!
//! Credentials live at `~/.harmont/credentials.toml` with structure:
//! ```toml
//! [tokens]
//! "https://api.harmont.dev" = "the-token"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const CREDS_FILE: &str = "credentials.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct CredsFile {
    #[serde(default)]
    tokens: BTreeMap<String, String>,
}

fn creds_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".harmont").join(CREDS_FILE))
}

/// Stash `token` for `api_base`. Empty token clears the entry.
#[allow(dead_code, reason = "consumed by the `login` verb in a later cluster")]
pub(crate) fn save_token(api_base: &str, token: &str) {
    let Some(path) = creds_path() else { return };
    let mut creds = load_creds_file(&path);
    if token.is_empty() {
        creds.tokens.remove(api_base);
    } else {
        creds.tokens.insert(api_base.to_string(), token.to_string());
    }
    write_creds_file(&path, &creds);
}

/// Load the token for `api_base`. Prefers `HARMONT_API_TOKEN` from the
/// caller-provided env over the file entry.
#[allow(
    dead_code,
    reason = "consumed by the auth/verb modules in a later cluster"
)]
pub(crate) fn load_token(api_base: &str, env: &BTreeMap<String, String>) -> Option<String> {
    if let Some(t) = env.get("HARMONT_API_TOKEN") {
        if !t.is_empty() {
            return Some(t.clone());
        }
    }
    let path = creds_path()?;
    let creds = load_creds_file(&path);
    creds.tokens.get(api_base).cloned()
}

#[allow(dead_code, reason = "consumed by the `logout` verb in a later cluster")]
pub(crate) fn clear_token(api_base: &str) {
    save_token(api_base, "");
}

fn load_creds_file(path: &PathBuf) -> CredsFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_creds_file(path: &PathBuf, creds: &CredsFile) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(creds) {
        let _ = std::fs::write(path, content);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
    }
}
