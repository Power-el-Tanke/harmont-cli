//! Safe wrappers around the host functions imported via Extism's
//! `host_fn!` block. Plugin code calls these instead of touching
//! `extism-pdk` directly.

// The `extern "ExtismHost"` block below is FFI to host imports; calling
// those externs requires `unsafe`. The safe wrappers in this module are
// the whole point of the file.
#![allow(unsafe_code)]
// The `extism_pdk::*` wildcard pulls in `Json`, `host_fn`, and other items
// the `host_fn!` macro expansion expects to find in scope; enumerating them
// here would duplicate the PDK's internal contract.
#![allow(clippy::wildcard_imports)]
// Every wrapper below returns a value that the plugin obviously wants
// (`#[must_use]` on every getter is noise — the call sites are short and
// the patterns are immediately recognisable).
#![allow(clippy::must_use_candidate)]
// `should_cancel` deliberately maps over the Result to extract the cancel
// flag and falls back to `false` on host-fn error; `is_ok_and` would lose
// the intent ("treat host-fn failure as 'not cancelled'").
#![allow(clippy::map_unwrap_or)]

use extism_pdk::*;
use hm_plugin_protocol::host_abi::*;
use hm_plugin_protocol::{BuildEvent, StdStream};

#[host_fn]
extern "ExtismHost" {
    fn hm_log(level: Json<Level>, msg: String);
    fn hm_emit_step_log(stream: Json<StdStream>, bytes: Vec<u8>);
    fn hm_emit_event(event: Json<BuildEvent>);

    fn hm_kv_get(scope: Json<KvScope>, key: String) -> Json<Option<Vec<u8>>>;
    fn hm_kv_set(scope: Json<KvScope>, key: String, val: Vec<u8>);

    fn hm_archive_read(args: Json<ArchiveReadArgs>) -> Vec<u8>;
    fn hm_archive_total_size(id: Json<ArchiveId>) -> u64;
    fn hm_fs_read_config(rel_path: String) -> Json<Option<Vec<u8>>>;

    fn hm_unix_socket_connect(path: String) -> Json<SocketHandle>;
    fn hm_socket_write(args: Json<SocketWriteArgs>) -> u64;
    fn hm_socket_read(args: Json<SocketReadArgs>) -> Vec<u8>;
    fn hm_socket_close(h: Json<SocketHandle>);

    fn hm_keyring_get(args: Json<KeyringArgs>) -> Json<Option<String>>;
    fn hm_keyring_set(args: Json<KeyringSetArgs>);
    fn hm_keyring_delete(args: Json<KeyringArgs>);

    fn hm_tty_prompt(args: Json<TtyPromptArgs>) -> String;
    fn hm_tty_confirm(args: Json<TtyConfirmArgs>) -> bool;
    fn hm_browser_open(url: String) -> bool;
    fn hm_spawn_loopback(port: Json<Option<u16>>) -> Json<LoopbackHandle>;
    fn hm_loopback_recv(args: Json<LoopbackRecvArgs>) -> Json<Option<CallbackData>>;

    fn hm_should_cancel() -> u32;

    fn hm_build_event_emit(bytes: Vec<u8>);

    fn hm_write_stdout(bytes: Vec<u8>);
    fn hm_write_stderr(bytes: Vec<u8>);
}

pub use hm_plugin_protocol::ArchiveId;

// ─── Safe API used by plugin code ───────────────────────────────────────────

/// Log a diagnostic line into the host's tracing subscriber.
///
/// # Panics
/// Never panics — Extism propagates host-fn errors as `Err` values,
/// which we trap and ignore (logs are best-effort).
pub fn log(level: Level, msg: &str) {
    let _ = unsafe { hm_log(Json(level), msg.to_string()) };
}

pub fn emit_step_log(stream: StdStream, bytes: &[u8]) {
    let _ = unsafe { hm_emit_step_log(Json(stream), bytes.to_vec()) };
}

pub fn emit_event(event: BuildEvent) {
    let _ = unsafe { hm_emit_event(Json(event)) };
}

pub fn build_event_emit(event: &hm_plugin_protocol::BuildEvent) {
    let Ok(bytes) = serde_json::to_vec(event) else { return; };
    let _ = unsafe { hm_build_event_emit(bytes) };
}

pub fn kv_get(scope: KvScope, key: &str) -> Option<Vec<u8>> {
    let Json(v) = unsafe { hm_kv_get(Json(scope), key.into()) }.unwrap_or(Json(None));
    v
}

pub fn kv_set(scope: KvScope, key: &str, val: &[u8]) {
    let _ = unsafe { hm_kv_set(Json(scope), key.into(), val.to_vec()) };
}

pub fn archive_total_size(id: ArchiveId) -> u64 {
    unsafe { hm_archive_total_size(Json(id)) }.unwrap_or(0)
}

pub fn archive_read(id: ArchiveId, offset: u64, max: u64) -> Vec<u8> {
    unsafe { hm_archive_read(Json(ArchiveReadArgs { id, offset, max })) }.unwrap_or_default()
}

pub fn fs_read_config(rel_path: &str) -> Option<Vec<u8>> {
    let Json(v) = unsafe { hm_fs_read_config(rel_path.into()) }.unwrap_or(Json(None));
    v
}

pub fn unix_socket_connect(path: &str) -> Option<SocketHandle> {
    unsafe { hm_unix_socket_connect(path.into()) }
        .ok()
        .map(|Json(h)| h)
}

pub fn socket_write(h: SocketHandle, bytes: &[u8]) -> u64 {
    unsafe {
        hm_socket_write(Json(SocketWriteArgs {
            h,
            bytes: bytes.to_vec(),
        }))
    }
    .unwrap_or(0)
}

pub fn socket_read(h: SocketHandle, max: u64) -> Vec<u8> {
    unsafe { hm_socket_read(Json(SocketReadArgs { h, max })) }.unwrap_or_default()
}

pub fn socket_close(h: SocketHandle) {
    let _ = unsafe { hm_socket_close(Json(h)) };
}

pub fn keyring_get(service: &str, account: &str) -> Option<String> {
    let Json(v) = unsafe {
        hm_keyring_get(Json(KeyringArgs {
            service: service.into(),
            account: account.into(),
        }))
    }
    .unwrap_or(Json(None));
    v
}

pub fn keyring_set(service: &str, account: &str, secret: &str) {
    let _ = unsafe {
        hm_keyring_set(Json(KeyringSetArgs {
            service: service.into(),
            account: account.into(),
            secret: secret.into(),
        }))
    };
}

pub fn keyring_delete(service: &str, account: &str) {
    let _ = unsafe {
        hm_keyring_delete(Json(KeyringArgs {
            service: service.into(),
            account: account.into(),
        }))
    };
}

pub fn tty_prompt(msg: &str, mask: bool) -> String {
    unsafe {
        hm_tty_prompt(Json(TtyPromptArgs {
            msg: msg.into(),
            mask,
        }))
    }
    .unwrap_or_default()
}

pub fn tty_confirm(msg: &str, default: bool) -> bool {
    unsafe {
        hm_tty_confirm(Json(TtyConfirmArgs {
            msg: msg.into(),
            default,
        }))
    }
    .unwrap_or(default)
}

pub fn browser_open(url: &str) -> bool {
    unsafe { hm_browser_open(url.into()) }.unwrap_or(false)
}

pub fn write_stdout(bytes: &[u8]) {
    let _ = unsafe { hm_write_stdout(bytes.to_vec()) };
}

pub fn write_stderr(bytes: &[u8]) {
    let _ = unsafe { hm_write_stderr(bytes.to_vec()) };
}

pub fn spawn_loopback(port: Option<u16>) -> Option<LoopbackHandle> {
    unsafe { hm_spawn_loopback(Json(port)) }
        .ok()
        .map(|Json(h)| h)
}

pub fn loopback_recv(h: LoopbackHandle, timeout_ms: u32) -> Option<CallbackData> {
    let Json(v) =
        unsafe { hm_loopback_recv(Json(LoopbackRecvArgs { h, timeout_ms })) }.unwrap_or(Json(None));
    v
}

pub fn should_cancel() -> bool {
    unsafe { hm_should_cancel() }
        .map(|n| n != 0)
        .unwrap_or(false)
}
