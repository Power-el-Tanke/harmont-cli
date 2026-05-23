//! Filesystem locations the plugin host inspects.

#![allow(clippy::must_use_candidate)]

use std::path::PathBuf;

pub fn user_plugins_dir() -> Option<PathBuf> {
    hm_util::dirs::harmont_user_plugins_dir()
}

pub fn project_plugins_dir() -> Option<PathBuf> {
    hm_util::dirs::harmont_project_plugins_dir()
}

pub fn install_dir() -> Option<PathBuf> {
    user_plugins_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::expect_used)]
    fn user_plugins_dir_resolves() {
        let p = user_plugins_dir().expect("home dir resolves");
        assert!(p.ends_with(".harmont/plugins"));
    }
}
