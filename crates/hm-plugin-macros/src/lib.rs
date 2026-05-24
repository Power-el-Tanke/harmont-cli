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
            other => Err(syn::Error::new(
                key.span(),
                format!(
                    "unknown keyword `{other}`. \
                     Expected one of: manifest, executor, hook, subcommand"
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
}

impl Parse for PluginArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut manifest: Option<Expr> = None;
        let mut executor: Option<Path> = None;
        let mut hook: Option<Path> = None;
        let mut subcommand: Option<Path> = None;

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
        })
    }
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

/// Generate struct fields for each registered capability.
fn gen_struct_fields(args: &PluginArgs) -> TokenStream2 {
    let executor_field = args.executor.as_ref().map(|ty| {
        quote! { executor: #ty, }
    });
    let hook_field = args.hook.as_ref().map(|ty| {
        quote! { hook: #ty, }
    });
    let subcommand_field = args.subcommand.as_ref().map(|ty| {
        quote! { subcommand: #ty, }
    });

    quote! {
        #executor_field
        #hook_field
        #subcommand_field
    }
}

/// Generate field initialisers (`field: <Ty as Default>::default()`) for
/// each registered capability.
fn gen_struct_init(args: &PluginArgs) -> TokenStream2 {
    let executor_init = args.executor.as_ref().map(|ty| {
        quote! { executor: <#ty as ::core::default::Default>::default(), }
    });
    let hook_init = args.hook.as_ref().map(|ty| {
        quote! { hook: <#ty as ::core::default::Default>::default(), }
    });
    let subcommand_init = args.subcommand.as_ref().map(|ty| {
        quote! { subcommand: <#ty as ::core::default::Default>::default(), }
    });

    quote! {
        #executor_init
        #hook_init
        #subcommand_init
    }
}

fn gen_execute_step(executor: Option<&Path>) -> TokenStream2 {
    executor.map_or_else(
        || gen_not_implemented_stub("execute_step", "input"),
        |_ty| {
            quote! {
                extern "C" fn execute_step<'a>(
                    &'a self,
                    input: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFutureUnsync<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    let executor = &self.executor;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::ExecutorInput =
                            match borsh::from_slice(input.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            borsh::to_vec(
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
                        match hm_plugin_sdk::StepExecutor::run(executor, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    borsh::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    borsh::to_vec(&e).unwrap_or_default(),
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
        |_ty| {
            quote! {
                extern "C" fn on_hook_event<'a>(
                    &'a self,
                    event: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFutureUnsync<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    let hook = &self.hook;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::HookEvent =
                            match borsh::from_slice(event.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            borsh::to_vec(
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
                        match hm_plugin_sdk::LifecycleHook::on_event(hook, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    borsh::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    borsh::to_vec(&e).unwrap_or_default(),
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
        |_ty| {
            quote! {
                extern "C" fn run_subcommand<'a>(
                    &'a self,
                    input: hm_plugin_sdk::ffi::FfiSlice<'a>,
                ) -> stabby::future::DynFutureUnsync<'a, hm_plugin_sdk::ffi::FfiResult> {
                    let ctx = &self.ctx;
                    let subcommand = &self.subcommand;
                    stabby::boxed::Box::new(async move {
                        let parsed: hm_plugin_sdk::SubcommandInput =
                            match borsh::from_slice(input.as_ref()) {
                                Ok(v) => v,
                                Err(e) => {
                                    return stabby::result::Result::Err(
                                        __ffi_bytes(
                                            borsh::to_vec(
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
                        match hm_plugin_sdk::SubcommandPlugin::run(subcommand, ctx, parsed).await {
                            Ok(r) => stabby::result::Result::Ok(
                                __ffi_bytes(
                                    borsh::to_vec(&r).unwrap_or_default(),
                                ),
                            ),
                            Err(e) => stabby::result::Result::Err(
                                __ffi_bytes(
                                    borsh::to_vec(&e).unwrap_or_default(),
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
        ) -> stabby::future::DynFutureUnsync<'a, hm_plugin_sdk::ffi::FfiResult> {
            let _ = #param_ident;
            stabby::boxed::Box::new(async {
                stabby::result::Result::Err(
                    __ffi_bytes(
                        borsh::to_vec(&hm_plugin_sdk::PluginError::new(
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

    let struct_fields = gen_struct_fields(args);
    let struct_init = gen_struct_init(args);

    let execute_step = gen_execute_step(args.executor.as_ref());
    let on_hook_event = gen_on_hook_event(args.hook.as_ref());
    let run_subcommand = gen_run_subcommand(args.subcommand.as_ref());

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
                #struct_fields
            }

            impl hm_plugin_sdk::ffi::RawPlugin for __HmPluginImpl {
                extern "C" fn manifest(&self) -> hm_plugin_sdk::ffi::FfiBytes {
                    self.manifest_bytes.clone()
                }

                #execute_step
                #on_hook_event
                #run_subcommand
            }

            // SAFETY: __HmPluginImpl holds a PluginContext (which is
            // Send + Sync) and FfiBytes (which is Send + Sync).
            // Capability types must also be Send + Sync (enforced by
            // the trait bounds on StepExecutor, LifecycleHook, etc.).
            unsafe impl Send for __HmPluginImpl {}
            unsafe impl Sync for __HmPluginImpl {}

            #[stabby::export]
            extern "C" fn hm_load_plugin(
                ctx: #host_ref,
            ) -> stabby::result::Result<#plugin_dyn, hm_plugin_sdk::ffi::FfiBytes> {
                let context = hm_plugin_sdk::PluginContext::new(ctx);
                let manifest_bytes: hm_plugin_sdk::ffi::FfiBytes =
                    __ffi_bytes(
                        borsh::to_vec(&{ #manifest_expr })
                            .expect("manifest serialization should never fail"),
                    );
                let plugin = __HmPluginImpl {
                    ctx: context,
                    manifest_bytes,
                    #struct_init
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
#[proc_macro]
pub fn hm_plugin(input: TokenStream) -> TokenStream {
    let args = syn::parse_macro_input!(input as PluginArgs);
    expand(&args).into()
}
