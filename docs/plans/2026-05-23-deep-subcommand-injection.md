# Deep Subcommand Injection Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the host parse plugin subcommand arguments using clap, inject plugin commands into the top-level CLI for help/completion, and pass structured JSON args to plugins instead of raw argv.

**Architecture:** Replace `SubcommandSpec.args_schema: ClapJson` (currently unused `serde_json::Value`) with a recursive `ArgSpec` tree that the host can convert into a `clap::Command` at runtime. The host builds the full CLI tree at startup, clap parses everything, and plugins receive `SubcommandInput.args` as structured JSON instead of `Null`. Plugins no longer need clap as a dependency — they deserialize args from JSON.

**Tech Stack:** clap 4 (dynamic `Command` builder API), serde_json, existing stabby FFI.

---

### Task 1: Define `ArgSpec` schema in `hm-plugin-protocol`

Replace the opaque `ClapJson` type alias with a concrete `ArgSpec` enum that describes arguments declaratively. This is the contract between plugin manifests and the host's clap builder.

**Files:**
- Modify: `crates/hm-plugin-protocol/src/manifest.rs`

**Step 1: Write the failing test**

Add to the existing `mod tests` block in `manifest.rs`:

```rust
#[test]
fn arg_spec_round_trips_through_json() {
    let spec = ArgSpec::Positional {
        name: "slug".into(),
        help: Some("Organization slug".into()),
        required: true,
        value_type: ValueType::String,
    };
    let json = serde_json::to_string(&spec).unwrap();
    let back: ArgSpec = serde_json::from_str(&json).unwrap();
    assert_eq!(spec, back);
}

#[test]
fn subcommand_spec_with_arg_specs_serializes() {
    let spec = SubcommandSpec {
        verb: "cloud".into(),
        about: "Cloud API".into(),
        args: vec![],
        subcommands: vec![SubcommandSpec {
            verb: "login".into(),
            about: "Authenticate".into(),
            args: vec![ArgSpec::Flag {
                long: "paste".into(),
                short: None,
                help: Some("Skip loopback".into()),
            }],
            subcommands: vec![],
        }],
    };
    let json = serde_json::to_value(&spec).unwrap();
    assert_eq!(json["subcommands"][0]["verb"], "login");
    assert_eq!(json["subcommands"][0]["args"][0]["long"], "paste");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hm-plugin-protocol -- arg_spec`
Expected: FAIL — `ArgSpec` not defined.

**Step 3: Define `ArgSpec`, `ValueType`, and update `SubcommandSpec`**

Replace the `ClapJson` type alias and update `SubcommandSpec`:

```rust
/// Describes one CLI argument the host should parse on the plugin's behalf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgSpec {
    /// A positional argument (e.g., `<slug>`).
    Positional {
        name: String,
        help: Option<String>,
        required: bool,
        value_type: ValueType,
    },
    /// A named option (e.g., `--pipeline <NAME>`).
    Option {
        long: String,
        short: Option<char>,
        help: Option<String>,
        required: bool,
        value_type: ValueType,
        default: Option<String>,
    },
    /// A boolean flag (e.g., `--paste`).
    Flag {
        long: String,
        short: Option<char>,
        help: Option<String>,
    },
}

/// The expected value type for an argument. The host validates and the
/// plugin deserializes the JSON value accordingly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueType {
    String,
    Int,
    Bool,
}
```

Update `SubcommandSpec`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, DeriveJsonSchema)]
pub struct SubcommandSpec {
    pub verb: String,
    pub about: String,
    /// Arguments this subcommand accepts. The host builds a clap
    /// `Command` from these and passes parsed values as JSON.
    pub args: Vec<ArgSpec>,
    pub subcommands: Vec<Self>,
}
```

Remove the `ClapJson` type alias. Remove the `args_schema` field.

**Step 4: Run test to verify it passes**

Run: `cargo test -p hm-plugin-protocol -- arg_spec`
Expected: PASS

**Step 5: Fix all compile errors from removing `args_schema`**

Every plugin manifest that references `args_schema` must change to `args: vec![]`. Files:
- `crates/hm/plugins/hm-plugin-cloud/src/lib.rs`: `args_schema: serde_json::json!({})` → `args: vec![]`
- `tests/fixtures/failing-subcommand/src/lib.rs`: `args_schema: serde_json::json!({"args": []})` → `args: vec![]`
- `tests/fixtures/host-fn-probe/src/lib.rs`: same change
- Any other fixture that references `args_schema`

Also remove the `ClapJson` re-export from `crates/hm-plugin-protocol/src/lib.rs` if present.

Run: `cargo check --workspace`
Expected: clean

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(protocol): replace ClapJson with typed ArgSpec schema"
```

---

### Task 2: Build `clap::Command` from `SubcommandSpec` tree

Add a module in `hm-plugin-runtime` that converts a `SubcommandSpec` tree into a `clap::Command`, and a function that extracts parsed matches back into `serde_json::Value`.

**Files:**
- Create: `crates/hm-plugin-runtime/src/clap_bridge.rs`
- Modify: `crates/hm-plugin-runtime/src/lib.rs` (add `pub mod clap_bridge`)
- Modify: `crates/hm-plugin-runtime/Cargo.toml` (add `clap = { version = "4", features = ["derive"] }`)

**Step 1: Write the failing test**

In `crates/hm-plugin-runtime/src/clap_bridge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use hm_plugin_protocol::manifest::{ArgSpec, SubcommandSpec, ValueType};

    fn cloud_spec() -> SubcommandSpec {
        SubcommandSpec {
            verb: "cloud".into(),
            about: "Cloud API".into(),
            args: vec![],
            subcommands: vec![
                SubcommandSpec {
                    verb: "login".into(),
                    about: "Authenticate".into(),
                    args: vec![ArgSpec::Flag {
                        long: "paste".into(),
                        short: None,
                        help: Some("Skip loopback".into()),
                    }],
                    subcommands: vec![],
                },
                SubcommandSpec {
                    verb: "org".into(),
                    about: "Manage orgs".into(),
                    args: vec![],
                    subcommands: vec![SubcommandSpec {
                        verb: "switch".into(),
                        about: "Set active org".into(),
                        args: vec![ArgSpec::Positional {
                            name: "slug".into(),
                            help: Some("Organization slug".into()),
                            required: true,
                            value_type: ValueType::String,
                        }],
                        subcommands: vec![],
                    }],
                },
            ],
        }
    }

    #[test]
    fn builds_clap_command_from_spec() {
        let cmd = build_command(&cloud_spec());
        // Should have "login" and "org" subcommands
        let subs: Vec<&str> = cmd
            .get_subcommands()
            .map(|c| c.get_name())
            .collect();
        assert!(subs.contains(&"login"));
        assert!(subs.contains(&"org"));
    }

    #[test]
    fn parses_flag_subcommand() {
        let cmd = build_command(&cloud_spec());
        let matches = cmd.try_get_matches_from(["cloud", "login", "--paste"]).unwrap();
        let (verb_path, args) = extract_args(&matches);
        assert_eq!(verb_path, vec!["cloud", "login"]);
        assert_eq!(args["paste"], true);
    }

    #[test]
    fn parses_nested_positional() {
        let cmd = build_command(&cloud_spec());
        let matches = cmd.try_get_matches_from(["cloud", "org", "switch", "acme"]).unwrap();
        let (verb_path, args) = extract_args(&matches);
        assert_eq!(verb_path, vec!["cloud", "org", "switch"]);
        assert_eq!(args["slug"], "acme");
    }

    #[test]
    fn missing_required_arg_errors() {
        let cmd = build_command(&cloud_spec());
        assert!(cmd.try_get_matches_from(["cloud", "org", "switch"]).is_err());
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hm-plugin-runtime -- clap_bridge`
Expected: FAIL — module doesn't exist.

**Step 3: Implement `build_command` and `extract_args`**

```rust
//! Converts plugin SubcommandSpec trees into clap Commands
//! and extracts parsed matches back into JSON.

use clap::{Arg, ArgAction, ArgMatches, Command};
use hm_plugin_protocol::manifest::{ArgSpec, SubcommandSpec, ValueType};

/// Build a `clap::Command` from a plugin's `SubcommandSpec`.
pub fn build_command(spec: &SubcommandSpec) -> Command {
    let mut cmd = Command::new(spec.verb.clone())
        .about(spec.about.clone())
        .disable_help_subcommand(true)
        .arg_required_else_help(!spec.subcommands.is_empty() && spec.args.is_empty());

    for arg_spec in &spec.args {
        cmd = cmd.arg(build_arg(arg_spec));
    }

    for sub in &spec.subcommands {
        cmd = cmd.subcommand(build_command(sub));
    }

    cmd
}

fn build_arg(spec: &ArgSpec) -> Arg {
    match spec {
        ArgSpec::Positional {
            name,
            help,
            required,
            ..
        } => {
            let mut arg = Arg::new(name.clone()).required(*required);
            if let Some(h) = help {
                arg = arg.help(h.clone());
            }
            arg
        }
        ArgSpec::Option {
            long,
            short,
            help,
            required,
            default,
            ..
        } => {
            let mut arg = Arg::new(long.clone())
                .long(long.clone())
                .required(*required);
            if let Some(s) = short {
                arg = arg.short(*s);
            }
            if let Some(h) = help {
                arg = arg.help(h.clone());
            }
            if let Some(d) = default {
                arg = arg.default_value(d.clone());
            }
            arg
        }
        ArgSpec::Flag {
            long, short, help, ..
        } => {
            let mut arg = Arg::new(long.clone())
                .long(long.clone())
                .action(ArgAction::SetTrue);
            if let Some(s) = short {
                arg = arg.short(*s);
            }
            if let Some(h) = help {
                arg = arg.help(h.clone());
            }
            arg
        }
    }
}

/// Walk the matched subcommand chain and collect verb_path + args JSON.
pub fn extract_args(matches: &ArgMatches) -> (Vec<String>, serde_json::Value) {
    let mut verb_path = Vec::new();
    let mut current = matches;

    // The top-level command name isn't in matches, caller provides it
    // via the spec. Walk into subcommands:
    loop {
        if let Some((name, sub)) = current.subcommand() {
            verb_path.push(name.to_string());
            current = sub;
        } else {
            break;
        }
    }

    let args = extract_match_args(current);
    (verb_path, args)
}

fn extract_match_args(matches: &ArgMatches) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for id in matches.ids() {
        let id_str = id.as_str();
        if let Some(values) = matches.try_get_raw(id_str).ok().flatten() {
            let strs: Vec<&str> = values
                .filter_map(|v| v.to_str().ok())
                .collect();
            if strs.len() == 1 {
                map.insert(id_str.into(), serde_json::Value::String(strs[0].into()));
            }
        } else if let Ok(true) = matches.try_get_one::<bool>(id_str) {
            map.insert(id_str.into(), serde_json::Value::Bool(true));
        }
    }
    serde_json::Value::Object(map)
}
```

Note: The exact `extract_match_args` implementation may need refinement — clap's `ArgMatches` API for dynamic commands requires care. The tests will validate correctness. Iterate until tests pass.

**Step 4: Run test to verify it passes**

Run: `cargo test -p hm-plugin-runtime -- clap_bridge`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(runtime): clap_bridge — build Command from SubcommandSpec, extract parsed args"
```

---

### Task 3: Inject plugin subcommands into top-level CLI

Wire the clap bridge into `cli/mod.rs` so plugin subcommands appear in `hm --help` and are parsed by clap directly, replacing the `#[command(external_subcommand)]` fallback.

**Files:**
- Modify: `crates/hm/src/cli/mod.rs`
- Modify: `crates/hm/src/cli/external.rs`
- Modify: `crates/hm/src/main.rs` (if CLI construction is there)

**Step 1: Understand the current entry point**

Read `crates/hm/src/main.rs` to see how `Cli::parse()` is called. The key insight: we need to replace `Cli::parse()` (which uses the derive API) with a two-phase approach:
1. Load the plugin registry to discover subcommand specs
2. Build the CLI `Command` with plugin subcommands appended
3. Parse argv against the augmented command
4. Route to built-in handlers or plugin dispatch

**Step 2: Add a function that augments the clap Command**

In `crates/hm/src/cli/mod.rs`, add:

```rust
use hm_plugin_runtime::clap_bridge;

/// Append plugin-provided subcommands to the base CLI command.
pub fn augment_with_plugins(
    mut cmd: clap::Command,
    specs: &[(String, SubcommandSpec)],  // (plugin_name, spec)
) -> clap::Command {
    for (_, spec) in specs {
        cmd = cmd.subcommand(clap_bridge::build_command(spec));
    }
    cmd
}
```

**Step 3: Change CLI dispatch to use augmented command**

Replace `Cli::parse()` with:

```rust
// 1. Build base command from derive
let base_cmd = Cli::command();

// 2. Load plugin registry (discovery only, no full host API needed)
let registry = PluginRegistry::load(RegistryConfig {
    auto_discover: true,
    ..Default::default()
})?;

// 3. Collect subcommand specs from plugin manifests
let plugin_specs: Vec<(String, SubcommandSpec)> = registry
    .manifests()
    .flat_map(|m| m.capabilities.iter().filter_map(|c| match c {
        Capability::Subcommand(s) => Some((m.name.clone(), s.clone())),
        _ => None,
    }))
    .collect();

// 4. Augment and parse
let augmented = augment_with_plugins(base_cmd, &plugin_specs);
let matches = augmented.get_matches();

// 5. Route: check if it's a built-in command (derive-parse) or a plugin subcommand
```

**Step 4: Update the dispatch logic**

The tricky part: built-in commands (`run`, `version`, `plugin`, `dev`) still use the derive-parsed `Cli` struct. Plugin subcommands use the dynamic `ArgMatches`.

Approach: Try derive-parsing first. If the subcommand isn't recognized by the derive parser, check if it matches a plugin verb and route through `external::run` with the parsed `ArgMatches`.

Change `external::run` signature:

```rust
// OLD:
pub async fn run(argv: Vec<String>) -> Result<i32>

// NEW:
pub async fn run(
    verb: &str,
    verb_path: Vec<String>,
    args: serde_json::Value,
    registry: &PluginRegistry,
) -> Result<i32>
```

The host now passes structured args instead of raw argv.

**Step 5: Remove `#[command(external_subcommand)]`**

Delete the `External(Vec<String>)` variant from `Command` enum — no longer needed since all subcommands are now in the clap tree.

**Step 6: Verify `hm --help` shows plugin subcommands**

Run: `cargo run -- --help`
Expected: Output includes `cloud  Talk to the Harmont cloud API` alongside built-in commands.

**Step 7: Commit**

```bash
git add -A
git commit -m "feat(cli): inject plugin subcommands into clap, host-side arg parsing"
```

---

### Task 4: Update `SubcommandInput` flow and plugin-side consumption

Update the host to populate `SubcommandInput.args` with real parsed JSON, and update plugins to consume structured args instead of parsing raw argv.

**Files:**
- Modify: `crates/hm/src/cli/external.rs`
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/lib.rs`
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/cli.rs`

**Step 1: Write a test for the cloud plugin receiving parsed args**

In an integration test or the cloud plugin's test module:

```rust
#[tokio::test]
async fn cloud_login_receives_parsed_args() {
    let input = SubcommandInput {
        verb_path: vec!["cloud".into(), "login".into()],
        args: serde_json::json!({"paste": true}),
        env: BTreeMap::new(),
    };
    // The plugin should be able to dispatch from structured args
    // without needing to parse raw argv
}
```

**Step 2: Update cloud plugin to dispatch from `SubcommandInput.args`**

Replace the clap parsing in `cli.rs` with JSON deserialization. The plugin's `dispatch` function changes:

```rust
// OLD: parse raw argv with clap
pub(crate) async fn dispatch(
    ctx: &PluginContext<'_>,
    argv: Vec<String>,
    env: BTreeMap<String, String>,
) -> Result<ExitInfo, PluginError>

// NEW: route based on verb_path, deserialize args from JSON
pub(crate) async fn dispatch(
    ctx: &PluginContext<'_>,
    input: SubcommandInput,
) -> Result<ExitInfo, PluginError> {
    let verb = input.verb_path.last().map(String::as_str).unwrap_or("");
    match verb {
        "login" => {
            let paste = input.args.get("paste")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            auth::login::run(ctx, &input.env, paste).await
        }
        "logout" => auth::logout::run(ctx, &input.env).await,
        // ... etc
    }
}
```

**Step 3: Update `Cloud`'s `SubcommandPlugin::run` impl**

```rust
impl SubcommandPlugin for Cloud {
    fn run<'a>(
        &'a self,
        ctx: &'a PluginContext<'a>,
        input: SubcommandInput,
    ) -> impl Future<Output = Result<ExitInfo, PluginError>> + Send + 'a {
        async move { cli::dispatch(ctx, input).await }
    }
}
```

**Step 4: Update the cloud plugin manifest with real `ArgSpec`s**

The manifest must now declare the full arg tree:

```rust
capabilities: vec![Capability::Subcommand(SubcommandSpec {
    verb: "cloud".into(),
    about: "Talk to the Harmont cloud API".into(),
    args: vec![],
    subcommands: vec![
        SubcommandSpec {
            verb: "login".into(),
            about: "Authenticate this CLI against the Harmont API".into(),
            args: vec![ArgSpec::Flag {
                long: "paste".into(),
                short: None,
                help: Some("Skip the loopback flow and prompt for a paste-in code".into()),
            }],
            subcommands: vec![],
        },
        SubcommandSpec {
            verb: "logout".into(),
            about: "Remove stored credentials".into(),
            args: vec![],
            subcommands: vec![],
        },
        // ... all other subcommands with their ArgSpecs
    ],
})],
```

This is verbose. Task 6 adds an SDK helper macro to generate this from clap derives. For now, write it out manually for the cloud plugin.

**Step 5: Remove clap dependency from cloud plugin**

In `crates/hm/plugins/hm-plugin-cloud/Cargo.toml`, remove:
```toml
clap = { ... }
```

Delete `CloudCli`, `CloudCommand`, and all clap derive types from `cli.rs`.

**Step 6: Run integration tests**

Run: `cargo test --workspace`
Expected: PASS — cloud plugin dispatches from structured args.

**Step 7: Commit**

```bash
git add -A
git commit -m "feat(cloud): consume parsed args from host instead of raw argv"
```

---

### Task 5: Update test fixture plugins

Update `failing-subcommand` and `host-fn-probe` fixtures to use the new `args: vec![]` manifest format. These are simple — they ignore args entirely, so no dispatch changes needed.

**Files:**
- Modify: `tests/fixtures/failing-subcommand/src/lib.rs`
- Modify: `tests/fixtures/host-fn-probe/src/lib.rs`
- Modify: any other fixtures with `SubcommandSpec`

**Step 1: Update manifests**

Already done in Task 1 Step 5 (compile fix). Verify the fixtures still build and tests pass.

**Step 2: Run all integration tests**

Run: `cargo test -p harmont-cli --test plugin_host_fns --test plugin_manifest --test plugin_registry --test runner_dispatch`
Expected: PASS

**Step 3: Commit** (if any changes needed beyond Task 1)

```bash
git add -A
git commit -m "test: update fixture plugins for ArgSpec manifest format"
```

---

### Task 6: SDK helper to generate `SubcommandSpec` from clap derives

Plugin authors shouldn't hand-write `ArgSpec` trees. Add an SDK function that introspects a clap `Command` (built from `#[derive(Parser)]`) and produces a `SubcommandSpec`.

**Files:**
- Create: `crates/hm-plugin-sdk/src/spec_from_clap.rs`
- Modify: `crates/hm-plugin-sdk/src/lib.rs` (add `pub mod spec_from_clap`)

**Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Debug, Parser)]
    #[command(name = "example", about = "Example plugin")]
    struct ExampleCli {
        #[command(subcommand)]
        command: ExampleCommand,
    }

    #[derive(Debug, Subcommand)]
    enum ExampleCommand {
        /// Do the thing.
        DoIt {
            /// Target name.
            name: String,
            /// Dry run.
            #[arg(long)]
            dry_run: bool,
        },
    }

    #[test]
    fn generates_spec_from_clap_command() {
        let cmd = ExampleCli::command();
        let spec = spec_from_command(&cmd);
        assert_eq!(spec.verb, "example");
        assert_eq!(spec.subcommands.len(), 1);
        assert_eq!(spec.subcommands[0].verb, "do-it");
        assert_eq!(spec.subcommands[0].args.len(), 2);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p hm-plugin-sdk -- spec_from_clap`
Expected: FAIL

**Step 3: Implement `spec_from_command`**

```rust
use clap::Command;
use hm_plugin_protocol::manifest::{ArgSpec, SubcommandSpec, ValueType};

/// Build a `SubcommandSpec` by introspecting a clap `Command`.
/// Use this in `hm_plugin!` to generate the manifest from your
/// clap derive types automatically.
pub fn spec_from_command(cmd: &Command) -> SubcommandSpec {
    let args: Vec<ArgSpec> = cmd
        .get_arguments()
        .filter(|a| a.get_id() != "help" && a.get_id() != "version")
        .map(arg_spec_from_clap_arg)
        .collect();

    let subcommands: Vec<SubcommandSpec> = cmd
        .get_subcommands()
        .filter(|c| c.get_name() != "help")
        .map(spec_from_command)
        .collect();

    SubcommandSpec {
        verb: cmd.get_name().to_string(),
        about: cmd.get_about().map_or_else(String::new, |s| s.to_string()),
        args,
        subcommands,
    }
}

fn arg_spec_from_clap_arg(arg: &clap::Arg) -> ArgSpec {
    let is_flag = arg.get_action().is_set_true()
        || arg.get_action().is_count();
    let is_positional = arg.get_long().is_none() && arg.get_short().is_none();

    if is_flag {
        ArgSpec::Flag {
            long: arg.get_long().unwrap_or(arg.get_id().as_str()).to_string(),
            short: arg.get_short(),
            help: arg.get_help().map(|s| s.to_string()),
        }
    } else if is_positional {
        ArgSpec::Positional {
            name: arg.get_id().to_string(),
            help: arg.get_help().map(|s| s.to_string()),
            required: arg.is_required_set(),
            value_type: ValueType::String,
        }
    } else {
        ArgSpec::Option {
            long: arg.get_long().unwrap_or(arg.get_id().as_str()).to_string(),
            short: arg.get_short(),
            help: arg.get_help().map(|s| s.to_string()),
            required: arg.is_required_set(),
            value_type: ValueType::String,
            default: arg.get_default_values().first().map(|v| v.to_str().unwrap_or("").to_string()),
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p hm-plugin-sdk -- spec_from_clap`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "feat(sdk): spec_from_command — generate SubcommandSpec from clap Command"
```

---

### Task 7: Migrate cloud plugin to use `spec_from_command`

Replace the hand-written `SubcommandSpec` tree in the cloud plugin manifest with the SDK helper, keeping clap as a dev/build dependency only for manifest generation.

**Files:**
- Modify: `crates/hm/plugins/hm-plugin-cloud/src/lib.rs`

**Step 1: Generate the spec from the existing clap types**

Since the cloud plugin no longer parses argv at runtime (Task 4 removed that), the clap derive types can move to a `manifest` module used only for spec generation:

```rust
// In lib.rs, the manifest generation:
use hm_plugin_sdk::spec_from_clap::spec_from_command;

// Keep the clap derives just for spec generation
mod manifest_schema {
    use clap::{Parser, Subcommand};
    // ... CloudCli, CloudCommand, etc. (the clap derive types)
}

hm_plugin!(
    manifest = PluginManifest {
        // ...
        capabilities: vec![Capability::Subcommand(
            spec_from_command(&manifest_schema::CloudCli::command())
        )],
        // ...
    },
    subcommand = Cloud,
);
```

This way the clap types define the schema once, the SDK helper converts to `SubcommandSpec` for the manifest, and the host parses args at runtime.

**Step 2: Verify integration tests pass**

Run: `cargo test --workspace`
Expected: PASS

**Step 3: Commit**

```bash
git add -A
git commit -m "refactor(cloud): generate SubcommandSpec from clap derives via SDK helper"
```

---

## Verification

1. `cargo check --workspace` — clean compile
2. `cargo test --workspace` — all tests pass
3. `cargo run -- --help` — shows plugin subcommands (e.g., `cloud`)
4. `cargo run -- cloud --help` — shows cloud sub-subcommands with help from ArgSpec
5. `cargo run -- cloud login --paste` — host parses `--paste`, plugin receives `{"paste": true}`
6. `cargo run -- cloud org switch acme` — host parses positional, plugin receives `{"slug": "acme"}`
