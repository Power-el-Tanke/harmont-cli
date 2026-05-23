#![allow(unsafe_code)]

use stabby::future::DynFutureUnsync;

pub type FfiBytes = stabby::vec::Vec<u8>;
pub type FfiSlice<'a> = stabby::slice::Slice<'a, u8>;
pub type FfiResult = stabby::result::Result<FfiBytes, FfiBytes>;

#[stabby::stabby]
pub trait RawPlugin: Send + Sync {
    extern "C" fn manifest(&self) -> FfiBytes;
    extern "C" fn execute_step<'a>(&'a self, input: FfiSlice<'a>) -> DynFutureUnsync<'a, FfiResult>;
    extern "C" fn on_hook_event<'a>(&'a self, event: FfiSlice<'a>) -> DynFutureUnsync<'a, FfiResult>;
    extern "C" fn run_subcommand<'a>(&'a self, input: FfiSlice<'a>) -> DynFutureUnsync<'a, FfiResult>;
}

#[stabby::stabby]
pub trait RawHostApi: Send + Sync {
    extern "C" fn log(&self, level: u8, msg: FfiSlice<'_>);
    extern "C" fn kv_get(&self, scope: u8, key: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
    extern "C" fn kv_set(&self, scope: u8, key: FfiSlice<'_>, val: FfiSlice<'_>);
    extern "C" fn emit_event(&self, event_borsh: FfiSlice<'_>);
    extern "C" fn emit_step_log(&self, stream: u8, bytes: FfiSlice<'_>);
    extern "C" fn should_cancel(&self) -> bool;
    extern "C" fn write_stdout(&self, bytes: FfiSlice<'_>);
    extern "C" fn write_stderr(&self, bytes: FfiSlice<'_>);
    extern "C" fn archive_read(&self, id_borsh: FfiSlice<'_>, offset: u64, max: u64) -> FfiBytes;
    extern "C" fn archive_total_size(&self, id_borsh: FfiSlice<'_>) -> u64;
    extern "C" fn fs_read_config(&self, rel_path: FfiSlice<'_>) -> stabby::option::Option<FfiBytes>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn _assert_raw_plugin_object_safe(_: stabby::Dyn<'_, stabby::boxed::Box<()>, stabby::vtable!(RawPlugin + Send + Sync)>) {}
    fn _assert_raw_host_api_object_safe(_: stabby::Dyn<'_, stabby::boxed::Box<()>, stabby::vtable!(RawHostApi + Send + Sync)>) {}
}
