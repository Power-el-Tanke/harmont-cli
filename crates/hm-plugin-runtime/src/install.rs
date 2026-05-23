//! Implementation of `hm plugin install <source> --pin <sha256>`.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::host::LoadedPlugin;
use crate::host_api::HostApiImpl;

/// Install a plugin from a file path or HTTPS URL.
///
/// For HTTPS URLs, `--pin <sha256>` is required. The pin must equal
/// the SHA-256 of the downloaded bytes (hex, lowercase).
///
/// On success, the plugin is written to
/// `<user-plugins-dir>/<manifest-name>.<dll-ext>`.
///
/// # Errors
///
/// Returns an error if the source cannot be fetched, the pin does not
/// verify, the plugin manifest fails validation, or the install dir
/// cannot be written to.
pub async fn install(source: &str, pin: Option<&str>) -> Result<PathBuf> {
    let bytes = if source.starts_with("https://") {
        let pin = pin.context("--pin <sha256> is required for HTTPS sources")?;
        let body = reqwest::get(source)
            .await
            .with_context(|| format!("GET {source}"))?
            .error_for_status()?
            .bytes()
            .await
            .context("read response body")?;
        verify_pin(&body, pin)?;
        body.to_vec()
    } else if source.starts_with("http://") {
        bail!("plain http:// is not allowed; use https:// or a local file path");
    } else {
        let path = PathBuf::from(source);
        if !path.is_file() {
            bail!("no file at {}", path.display());
        }
        let bytes = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        if let Some(pin) = pin {
            verify_pin(&bytes, pin)?;
        }
        bytes
    };

    let dll_ext = std::env::consts::DLL_EXTENSION;

    // Write to a temp file, load it to validate the manifest, then
    // move to the install dir with the manifest name.
    let tmp_dir = tempfile::tempdir().context("create tempdir for validation")?;
    let tmp_path = tmp_dir.path().join(format!("plugin.{dll_ext}"));
    std::fs::write(&tmp_path, &bytes)
        .with_context(|| format!("write temp {}", tmp_path.display()))?;

    let host_api = Arc::new(HostApiImpl::new_noop());
    let plugin = LoadedPlugin::load(&tmp_path, host_api)
        .context("validate plugin before installing")?;
    let name = plugin.manifest.name.clone();
    drop(plugin);

    let install_dir = hm_util::dirs::plugin_install_dir().context("resolve install dir")?;
    std::fs::create_dir_all(&install_dir)
        .with_context(|| format!("create {}", install_dir.display()))?;
    let target = install_dir.join(format!("{name}.{dll_ext}"));
    std::fs::write(&target, &bytes).with_context(|| format!("write {}", target.display()))?;
    Ok(target)
}

fn verify_pin(bytes: &[u8], expected_hex: &str) -> Result<()> {
    let mut h = Sha256::new();
    h.update(bytes);
    let got = h.finalize();
    let got_hex = hex::encode(got);
    if !got_hex.eq_ignore_ascii_case(expected_hex.trim()) {
        bail!(
            "SHA-256 mismatch: expected {expected_hex}, downloaded {got_hex}\n\
             fix: re-fetch the source or correct the --pin value"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn pin_verification_round_trip() {
        let body = b"hello plugin";
        let mut h = Sha256::new();
        h.update(body);
        let hex_digest = hex::encode(h.finalize());
        assert!(verify_pin(body, &hex_digest).is_ok());
        assert!(verify_pin(body, "deadbeef").is_err());
    }
}
