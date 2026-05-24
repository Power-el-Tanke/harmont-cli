//! ABI-stable dynamic value type, replacing `serde_json::Value` at
//! the plugin FFI boundary.
//!
//! # Why `#[repr(C, u8)]` instead of `#[stabby::stabby]`?
//!
//! `FfiValue` is a *recursive* enum — `Array` contains `Vec<FfiValue>`
//! and `Object` contains `Vec<FfiEntry>` which itself holds an
//! `FfiValue`.  stabby's `IStable` trait computes layout proof at
//! compile-time via associated-type chains.  Recursive types cause
//! infinite chains, hitting `E0275` ("overflow evaluating the
//! requirement").
//!
//! `#[repr(C, u8)]` gives a deterministic, C-ABI-compatible tagged
//! union.  Combined with stabby-stable inner types (`stabby::string::String`,
//! `stabby::vec::Vec`, etc.) the resulting layout is as stable as a
//! stabby-derived one — we just lose the compile-time `IStable` proof.

use stabby::vec::Vec as FfiVec;

/// A key-value pair used by [`FfiValue::Object`].
#[derive(Debug)]
#[repr(C)]
pub struct FfiEntry {
    pub key: stabby::string::String,
    pub value: FfiValue,
}

/// Dynamic value type for data whose schema is not known at compile
/// time: parsed CLI args, `runner_args`, JSON Schema fragments.
///
/// All variants use stabby-stable types so the in-memory layout is
/// ABI-stable across independently compiled cdylib plugins.
#[derive(Debug)]
#[repr(C, u8)]
pub enum FfiValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(stabby::string::String),
    Array(FfiVec<Self>),
    Object(FfiVec<FfiEntry>),
}
