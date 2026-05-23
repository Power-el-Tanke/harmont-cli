//! Converts [`SubcommandSpec`] trees into [`clap::Command`] objects
//! and extracts parsed [`clap::ArgMatches`] back into
//! [`serde_json::Value`].
//!
//! This lets plugins declare their CLI surface via data (the spec)
//! while the host owns all `clap` machinery.

use clap::{Arg, ArgAction, ArgMatches, Command};
use hm_plugin_protocol::manifest::{ArgSpec, SubcommandSpec, ValueType};

/// Recursively builds a [`clap::Command`] from a [`SubcommandSpec`] tree.
///
/// The returned command uses `spec.verb` as its name and `spec.about`
/// as the about string. Arguments and nested subcommands are mapped
/// one-to-one from the spec.
#[must_use]
pub fn build_command(spec: &SubcommandSpec) -> Command {
    let mut cmd = Command::new(spec.verb.clone()).about(spec.about.clone());

    for arg in &spec.args {
        cmd = cmd.arg(build_arg(arg));
    }

    for sub in &spec.subcommands {
        cmd = cmd.subcommand(build_command(sub));
    }

    // If the command has subcommands but no positional/option args of its
    // own, require a subcommand to be specified.
    if !spec.subcommands.is_empty() && spec.args.is_empty() {
        cmd = cmd.subcommand_required(true);
    }

    cmd
}

/// Walks the subcommand chain in `matches` to find the leaf command,
/// building up a `verb_path` of subcommand names along the way, and
/// then extracts all argument values at the leaf into a JSON map.
///
/// The returned `verb_path` does **not** include the root command name
/// because [`ArgMatches`] does not carry it. The caller (host dispatch)
/// must prepend the top-level verb if needed.
///
/// # Return value
///
/// `(verb_path, args)` where `verb_path` lists the subcommand names
/// from the root's immediate child down to the leaf, and `args` is a
/// JSON object whose keys are argument IDs.
#[must_use]
pub fn extract_args(matches: &ArgMatches) -> (Vec<String>, serde_json::Value) {
    let mut verb_path: Vec<String> = Vec::new();
    let mut current = matches;

    // Walk down the subcommand chain until we reach the leaf.
    while let Some((name, sub_matches)) = current.subcommand() {
        verb_path.push(name.to_owned());
        current = sub_matches;
    }

    let args = extract_leaf_args(current);
    (verb_path, args)
}

// ------------------------------------------------------------------
// Internal helpers
// ------------------------------------------------------------------

/// Argument IDs that clap inserts automatically; we skip these when
/// extracting values.
const BUILTIN_IDS: &[&str] = &["help", "version"];

fn build_arg(spec: &ArgSpec) -> Arg {
    match spec {
        ArgSpec::Positional {
            name,
            help,
            required,
            value_type,
        } => {
            let mut arg = Arg::new(name.clone()).required(*required);
            if *value_type == ValueType::Int {
                arg = arg.value_parser(clap::value_parser!(i64));
            }
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
            value_type,
            default,
        } => {
            let mut arg = Arg::new(long.clone())
                .long(long.clone())
                .required(*required);
            if *value_type == ValueType::Int {
                arg = arg.value_parser(clap::value_parser!(i64));
            }
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

fn extract_leaf_args(matches: &ArgMatches) -> serde_json::Value {
    let mut map = serde_json::Map::new();

    for id in matches.ids() {
        let id_str = id.as_str();
        if BUILTIN_IDS.contains(&id_str) {
            continue;
        }

        // Flags come back as bool via `ArgAction::SetTrue`.
        if let Ok(Some(&val)) = matches.try_get_one::<bool>(id_str) {
            map.insert(id_str.to_owned(), serde_json::Value::Bool(val));
            continue;
        }

        // Int-typed args come back as i64 via value_parser.
        if let Ok(Some(&val)) = matches.try_get_one::<i64>(id_str) {
            map.insert(
                id_str.to_owned(),
                serde_json::Value::Number(val.into()),
            );
            continue;
        }

        // Everything else (positionals, options) is a string.
        if let Ok(Some(val)) = matches.try_get_one::<String>(id_str) {
            map.insert(
                id_str.to_owned(),
                serde_json::Value::String(val.clone()),
            );
        }
    }

    serde_json::Value::Object(map)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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
        let subs: Vec<&str> = cmd.get_subcommands().map(|c| c.get_name()).collect();
        assert!(subs.contains(&"login"));
        assert!(subs.contains(&"org"));
    }

    #[test]
    fn parses_flag_subcommand() {
        let cmd = build_command(&cloud_spec());
        let matches = cmd
            .try_get_matches_from(["cloud", "login", "--paste"])
            .unwrap();
        let (verb_path, args) = extract_args(&matches);
        assert_eq!(verb_path, vec!["login"]);
        assert_eq!(args["paste"], true);
    }

    #[test]
    fn parses_nested_positional() {
        let cmd = build_command(&cloud_spec());
        let matches = cmd
            .try_get_matches_from(["cloud", "org", "switch", "acme"])
            .unwrap();
        let (verb_path, args) = extract_args(&matches);
        assert_eq!(verb_path, vec!["org", "switch"]);
        assert_eq!(args["slug"], "acme");
    }

    #[test]
    fn parses_option_with_int_value() {
        let spec = SubcommandSpec {
            verb: "billing".into(),
            about: "Billing".into(),
            args: vec![],
            subcommands: vec![SubcommandSpec {
                verb: "transactions".into(),
                about: "List transactions".into(),
                args: vec![ArgSpec::Option {
                    long: "limit".into(),
                    short: None,
                    help: None,
                    required: false,
                    value_type: ValueType::Int,
                    default: Some("100".into()),
                }],
                subcommands: vec![],
            }],
        };
        let cmd = build_command(&spec);
        let matches = cmd
            .try_get_matches_from(["billing", "transactions", "--limit", "50"])
            .unwrap();
        let (verb_path, args) = extract_args(&matches);
        assert_eq!(verb_path, vec!["transactions"]);
        assert_eq!(args["limit"], 50);
    }

    #[test]
    fn option_uses_default_when_omitted() {
        let spec = SubcommandSpec {
            verb: "billing".into(),
            about: "Billing".into(),
            args: vec![],
            subcommands: vec![SubcommandSpec {
                verb: "transactions".into(),
                about: "List transactions".into(),
                args: vec![ArgSpec::Option {
                    long: "limit".into(),
                    short: None,
                    help: None,
                    required: false,
                    value_type: ValueType::Int,
                    default: Some("100".into()),
                }],
                subcommands: vec![],
            }],
        };
        let cmd = build_command(&spec);
        let matches = cmd
            .try_get_matches_from(["billing", "transactions"])
            .unwrap();
        let (_, args) = extract_args(&matches);
        assert_eq!(args["limit"], 100);
    }

    #[test]
    fn missing_required_arg_errors() {
        let cmd = build_command(&cloud_spec());
        assert!(cmd
            .try_get_matches_from(["cloud", "org", "switch"])
            .is_err());
    }
}
