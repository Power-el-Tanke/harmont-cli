//! Persistent state (active organization slug) via file storage.

use serde::{Deserialize, Serialize};

const STATE_FILE: &str = "cloud-state.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct CloudState {
    pub active_org: Option<String>,
}

impl CloudState {
    pub(crate) fn load() -> Self {
        let Some(dir) = hm_util::dirs::harmont_config_dir() else {
            return Self::default();
        };
        let path = dir.join(STATE_FILE);
        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub(crate) fn save(&self) {
        let Some(dir) = hm_util::dirs::harmont_config_dir() else {
            return;
        };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(STATE_FILE);
        if let Ok(bytes) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(&path, bytes);
        }
    }
}
