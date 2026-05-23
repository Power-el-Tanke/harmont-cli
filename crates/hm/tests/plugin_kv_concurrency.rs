//! Concurrent writers to host-side KV must all win — the `HostApiImpl`
//! uses `std::sync::Mutex` so concurrent `kv_set` calls cannot lose writes.

#![allow(
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::sync::Arc;
use std::thread;

use harmont_cli::plugin::host_api::HostApiImpl;
use hm_plugin_sdk::ffi::RawHostApi;

#[test]
fn concurrent_kv_writes_all_persist() {
    const N: usize = 16;
    let host = Arc::new(HostApiImpl::new_noop());

    let handles: Vec<_> = (0..N)
        .map(|i| {
            let host = Arc::clone(&host);
            thread::spawn(move || {
                let key = format!("key_{i}");
                let val = vec![0x42u8; 1024];
                host.kv_set(
                    0, // KvScope::Plugin
                    hm_plugin_sdk::ffi::FfiSlice::from(key.as_bytes()),
                    hm_plugin_sdk::ffi::FfiSlice::from(val.as_slice()),
                );
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let missing: Vec<usize> = (0..N)
        .filter(|i| {
            let key = format!("key_{i}");
            let result = host.kv_get(0, hm_plugin_sdk::ffi::FfiSlice::from(key.as_bytes()));
            let std_result: core::option::Option<hm_plugin_sdk::ffi::FfiBytes> = result.into();
            std_result.is_none()
        })
        .collect();
    assert!(
        missing.is_empty(),
        "lost writes for keys: {missing:?} (got {} of {N})",
        N - missing.len()
    );
}
