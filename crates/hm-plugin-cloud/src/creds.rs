//! File-backed credential storage.

use std::collections::BTreeMap;
use std::io::Write;

const CREDS_FILE: &str = "credentials.toml";

pub(crate) fn save_token(api_base: &str, token: &str) {
    let Some(dir) = hm_util::dirs::harmont_config_dir() else {
        return;
    };
    let path = dir.join(CREDS_FILE);
    let mut table = load_table(&path);
    if token.is_empty() {
        table.remove(api_base);
    } else {
        table.insert(api_base.to_owned(), token.to_owned());
    }
    write_table(&path, &table);
}

pub(crate) fn load_token(api_base: &str, env: &BTreeMap<String, String>) -> Option<String> {
    if let Some(t) = env.get("HARMONT_API_TOKEN")
        && !t.is_empty()
    {
        return Some(t.clone());
    }
    let dir = hm_util::dirs::harmont_config_dir()?;
    let path = dir.join(CREDS_FILE);
    let table = load_table(&path);
    table.get(api_base).cloned()
}

pub(crate) fn clear_token(api_base: &str) {
    save_token(api_base, "");
}

fn load_table(path: &std::path::Path) -> BTreeMap<String, String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(val) = content.parse::<toml::Value>() else {
        return BTreeMap::new();
    };
    let Some(table) = val.as_table() else {
        return BTreeMap::new();
    };
    let mut map = BTreeMap::new();
    for (k, v) in table {
        if let Some(s) = v.as_str() {
            map.insert(k.clone(), s.to_owned());
        }
    }
    map
}

fn write_table(path: &std::path::Path, table: &BTreeMap<String, String>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut content = String::new();
    for (k, v) in table {
        content.push_str(&format!("{k} = {v:?}\n"));
    }
    if let Ok(mut f) = std::fs::File::create(path) {
        let _ = f.write_all(content.as_bytes());
        // Set 0600 on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
        }
    }
}
