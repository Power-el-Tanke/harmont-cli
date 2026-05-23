//! Proc-macro crate for `hm-plugin-sdk`.
//!
//! Provides the [`hm_plugin!`] macro that generates:
//! - `__HmPluginImpl` struct holding context and cached manifest bytes
//! - `impl RawPlugin for __HmPluginImpl` bridging FFI to async traits
//! - `#[stabby::export] fn hm_load_plugin(...)` entry point
//!
//! This crate is re-exported by `hm-plugin-sdk`; plugin authors write:
//!
//! ```ignore
//! hm_plugin!(
//!     manifest = PluginManifest { ... },
//!     executor = MyExec,
//! );
//! ```

// proc-macro crates cannot depend on runtime crates (stabby, hm-plugin-sdk).
// All generated code references those crates by their full paths.

// stabby macro expansions contain unsafe FFI code that we cannot avoid.
#![allow(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, Path, Token};

/// A single `key = value` pair in the macro invocation.
enum PluginArg {
    Manifest(Expr),
    Executor(Path),
    Hook(Path),
    Subcommand(Path),
    Output(Path),
}

impl Parse for PluginArg {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        let _eq: Token![=] = input.parse()?;

        match key.to_string().as_str() {
            "manifest" => {
                let expr: Expr = input.parse()?;
                Ok(Self::Manifest(expr))
            }
            "executor" => {
                let path: Path = input.parse()?;
                Ok(Self::Executor(path))
            }
            "hook" => {
                let path: Path = input.parse()?;
                Ok(Self::Hook(path))
            }
            "subcommand" => {
                let path: Path = input.parse()?;
                Ok(Self::Subcommand(path))
            }
            "output" => {
                let path: Path = input.parse()?;
                Ok(Self::Output(path))
            }
            other => Err(syn::Error::new(
                key.span(),
                format!(
                    "unknown keyword `{other}`. \
                     Expected one of: manifest, executor, hook, subcommand, output"
                ),
            )),
        }
    }
}

/// All parsed arguments from the `hm_plugin!(...)` invocation.
struct PluginArgs {
    manifest: Expr,
    executor: Option<Path>,
    hook: Option<Path>,
    subcommand: Option<Path>,
    output: Option<Path>,
}

impl Parse for PluginArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut manifest: Option<Expr> = None;
        let mut executor: Option<Path> = None;
        let mut hook: Option<Path> = None;
        let mut subcommand: Option<Path> = None;
        let mut output: Option<Path> = None;

        while !input.is_empty() {
            let arg: PluginArg = input.parse()?;
            match arg {
                PluginArg::Manifest(expr) => {
                    if manifest.is_some() {
                        return Err(syn::Error::new(
                            input.span(),
                            "duplicate `manifest` argument",
                        ));
                    }
                    manifest = Some(expr);
                }
                PluginArg::Executor(path) => {
                    if executor.is_some() {
                        return Err(syn::Error::new(
                            input.span(),
                            "duplicate `executor` argument",
                        ));
                    }
                    executor = Some(path);
                }
                PluginArg::Hook(path) => {
                    if hook.is_some() {
                        return Err(syn::Error::new(
                            input.span(),
                            "duplicate `hook` argument",
                        ));
                    }
                    hook = Some(path);
                }
                PluginArg::Subcommand(path) => {
                    if subcommand.is_some() {
                        return Err(syn::Error::new(
                            input.span(),
                            "duplicate `subcommand` argument",
                        ));
                    }
                    subcommand = Some(path);
                }
                PluginArg::Output(path) => {
                    if output.is_some() {
                        return Err(syn::Error::new(
                            input.span(),
                            "duplicate `output` argument",
                        ));
                    }
                    output = Some(path);
                }
            }
            // consume optional trailing comma
            let _ = input.parse::<Option<Token![,]>>();
        }

        let manifest = manifest.ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                "missing required `manifest` argument",
            )
        })?;

        Ok(Self {
            manifest,
            executor,
            hook,
            subcommand,
            output,
        })
    }
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

fn gen_execute_step(executor: Option<&Path>) -> TokenStream2 {
    executor.map_or_else(
        || gen_not_implemented_stub("execute_step", "input"),
        |ty| {
            quote! {
                extern "C" fn execute_step<'a>(
                    &'a self,
                    input: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::ExecutorInput =
                            match serde_json::from_slice(input.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            serde_json::to_vec(
                                                &hm_plugin_sdk::PluginError::new(
                                                    "deserialize",
                                                    e.to_string(),
                                                ),
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                }
                            };
                        let plugin = <#ty as ::core::default::Default>::default();
                        match hm_plugin_sdk::StepExecutor::run(&plugin, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    serde_json::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    serde_json::to_vec(&e).unwrap_or_default(),
                                ),
                            ),
                        }
                    })
                    .into()
                }
            }
        },
    )
}

fn gen_on_hook_event(hook: Option<&Path>) -> TokenStream2 {
    hook.map_or_else(
        || gen_not_implemented_stub("on_hook_event", "event"),
        |ty| {
            quote! {
                extern "C" fn on_hook_event<'a>(
                    &'a self,
                    event: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::HookEvent =
                            match serde_json::from_slice(event.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            serde_json::to_vec(
                                                &hm_plugin_sdk::PluginError::new(
                                                    "deserialize",
                                                    e.to_string(),
                                                ),
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                }
                            };
                        let plugin = <#ty as ::core::default::Default>::default();
                        match hm_plugin_sdk::LifecycleHook::on_event(&plugin, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    serde_json::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    serde_json::to_vec(&e).unwrap_or_default(),
                                ),
                            ),
                        }
                    })
                    .into()
                }
            }
        },
    )
}

fn gen_run_subcommand(subcommand: Option<&Path>) -> TokenStream2 {
    subcommand.map_or_else(
        || gen_not_implemented_stub("run_subcommand", "input"),
        |ty| {
            quote! {
                extern "C" fn run_subcommand<'a>(
                    &'a self,
                    input: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::SubcommandInput =
                            match serde_json::from_slice(input.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            serde_json::to_vec(
                                                &hm_plugin_sdk::PluginError::new(
                                                    "deserialize",
                                                    e.to_string(),
                                                ),
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                }
                            };
                        let plugin = <#ty as ::core::default::Default>::default();
                        match hm_plugin_sdk::SubcommandPlugin::run(&plugin, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    serde_json::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    serde_json::to_vec(&e).unwrap_or_default(),
                                ),
                            ),
                        }
                    })
                    .into()
                }
            }
        },
    )
}

fn gen_on_output_event(output: Option<&Path>) -> TokenStream2 {
    output.map_or_else(
        || gen_not_implemented_stub("on_output_event", "event"),
        |ty| {
            quote! {
                extern "C" fn on_output_event<'a>(
                    &'a self,
                    event: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::BuildEvent =
                            match serde_json::from_slice(event.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            serde_json::to_vec(
                                                &hm_plugin_sdk::PluginError::new(
                                                    "deserialize",
                                                    e.to_string(),
                                                ),
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                }
                            };
                        let plugin = <#ty as ::core::default::Default>::default();
                        match hm_plugin_sdk::OutputFormatter::on_event(&plugin, ctx, parsed).await {
                            Ok(()) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    serde_json::to_vec(&()).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    serde_json::to_vec(&e).unwrap_or_default(),
                                ),
                            ),
                        }
                    })
                    .into()
                }
            }
        },
    )
}

fn gen_finalize_output(output: Option<&Path>) -> TokenStream2 {
    output.map_or_else(
        || {
            quote! {
                extern "C" fn finalize_output<'a>(
                    &'a self,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    stabby::boxed::Box::new(async {
                        stabby::result::Result::Err(
                            __ffi_bytes(
                                serde_json::to_vec(&hm_plugin_sdk::PluginError::new(
                                    "not_implemented",
                                    "this plugin does not implement this capability",
                                ))
                                .unwrap_or_default(),
                            ),
                        )
                    })
                    .into()
                }
            }
        },
        |ty| {
            quote! {
                extern "C" fn finalize_output<'a>(
                    &'a self,
                ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    stabby::boxed::Box::new(async move {
                        let plugin = <#ty as ::core::default::Default>::default();
                        match hm_plugin_sdk::OutputFormatter::finalize(&plugin, ctx).await {
                            Ok(bytes) => stabby::result::Result::Ok(
                                __ffi_bytes(bytes),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    serde_json::to_vec(&e).unwrap_or_default(),
                                ),
                            ),
                        }
                    })
                    .into()
                }
            }
        },
    )
}

fn gen_not_implemented_stub(method_name: &str, param_name: &str) -> TokenStream2 {
    let method_ident = syn::Ident::new(method_name, proc_macro2::Span::call_site());
    let param_ident = syn::Ident::new(param_name, proc_macro2::Span::call_site());

    quote! {
        extern "C" fn #method_ident<'a>(
            &'a self,
            #param_ident: hm_plugin_sdk::ffi::FfiSlice<'a>,
        ) -> stabby::future::DynFuture<'a, hm_plugin_sdk::ffi::FfiResult> {
            let _ = #param_ident;
            stabby::boxed::Box::new(async {
                stabby::result::Result::Err(
                    __ffi_bytes(
                        serde_json::to_vec(&hm_plugin_sdk::PluginError::new(
                            "not_implemented",
                            "this plugin does not implement this capability",
                        ))
                        .unwrap_or_default(),
                    ),
                )
            })
            .into()
        }
    }
}

/// Type alias tokens for `HostRef<'static>` — the stabby `DynRef` that
/// wraps the host API trait object. Matches the definition in
/// `hm_plugin_sdk::context`.
fn host_ref_type() -> TokenStream2 {
    quote! {
        stabby::DynRef<
            'static,
            <dyn Sync as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                <dyn Send as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                    <dyn hm_plugin_sdk::ffi::RawHostApi as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                        stabby::abi::vtable::VtDrop,
                    >,
                >,
            >,
        >
    }
}

/// Type alias tokens for the returned
/// `Dyn<'static, Box<()>, vtable!(RawPlugin + Send + Sync)>`.
fn plugin_dyn_type() -> TokenStream2 {
    quote! {
        stabby::Dyn<
            'static,
            stabby::boxed::Box<()>,
            <dyn Sync as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                <dyn Send as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                    <dyn hm_plugin_sdk::ffi::RawPlugin as stabby::abi::vtable::CompoundVt<'static>>::Vt<
                        stabby::abi::vtable::VtDrop,
                    >,
                >,
            >,
        >
    }
}

/// Generate the complete macro expansion.
fn expand(args: &PluginArgs) -> TokenStream2 {
    let manifest_expr = &args.manifest;
    let host_ref = host_ref_type();
    let plugin_dyn = plugin_dyn_type();

    let execute_step = gen_execute_step(args.executor.as_ref());
    let on_hook_event = gen_on_hook_event(args.hook.as_ref());
    let run_subcommand = gen_run_subcommand(args.subcommand.as_ref());
    let on_output_event = gen_on_output_event(args.output.as_ref());
    let finalize_output = gen_finalize_output(args.output.as_ref());

    quote! {
        // Generated by hm_plugin! — do not edit.
        #[allow(unsafe_code, non_camel_case_types, clippy::all, clippy::pedantic, clippy::nursery)]
        const _: () = {
            use hm_plugin_sdk::ffi::RawPlugin as _;

            /// Convert a `std::vec::Vec<u8>` to `stabby::vec::Vec<u8>` (`FfiBytes`).
            /// stabby's `Vec` implements `From<&[T]>` but not `From<std::vec::Vec<T>>`.
            #[inline]
            fn __ffi_bytes(v: ::std::vec::Vec<u8>) -> hm_plugin_sdk::ffi::FfiBytes {
                hm_plugin_sdk::ffi::FfiBytes::from(v.as_slice())
            }

            struct __HmPluginImpl {
                ctx: hm_plugin_sdk::PluginContext<'static>,
                manifest_bytes: hm_plugin_sdk::ffi::FfiBytes,
            }

            impl hm_plugin_sdk::ffi::RawPlugin for __HmPluginImpl {
                extern "C" fn manifest(&self) -> hm_plugin_sdk::ffi::FfiBytes {
                    self.manifest_bytes.clone()
                }

                #execute_step
                #on_hook_event
                #run_subcommand
                #on_output_event
                #finalize_output
            }

            // SAFETY: __HmPluginImpl holds a PluginContext (which is
            // Send + Sync) and FfiBytes (which is Send + Sync).
            unsafe impl Send for __HmPluginImpl {}
            unsafe impl Sync for __HmPluginImpl {}

            #[stabby::export]
            extern "C" fn hm_load_plugin(
                ctx: #host_ref,
            ) -> stabby::result::Result<#plugin_dyn, hm_plugin_sdk::ffi::FfiBytes> {
                let context = hm_plugin_sdk::PluginContext::new(ctx);
                let manifest_bytes: hm_plugin_sdk::ffi::FfiBytes =
                    __ffi_bytes(
                        serde_json::to_vec(&{ #manifest_expr })
                            .expect("manifest serialization should never fail"),
                    );
                let plugin = __HmPluginImpl {
                    ctx: context,
                    manifest_bytes,
                };
                stabby::result::Result::Ok(
                    stabby::boxed::Box::new(plugin).into()
                )
            }
        };
    }
}

/// Generate the FFI glue for a native `hm` plugin.
///
/// # Usage
///
/// ```ignore
/// use hm_plugin_sdk::*;
///
/// hm_plugin!(
///     manifest = PluginManifest { /* ... */ },
///     executor = MyExec,
/// );
/// ```
///
/// Keyword arguments (order-independent, comma-separated):
///
/// | Keyword      | Required | Value type            |
/// |--------------|----------|-----------------------|
/// | `manifest`   | **yes**  | expression            |
/// | `executor`   | no       | type implementing `StepExecutor` |
/// | `hook`       | no       | type implementing `LifecycleHook` |
/// | `subcommand` | no       | type implementing `SubcommandPlugin` |
/// | `output`     | no       | type implementing `OutputFormatter` |
#[proc_macro]
pub fn hm_plugin(input: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(input as PluginArgs);
    expand(&args).into()
}
