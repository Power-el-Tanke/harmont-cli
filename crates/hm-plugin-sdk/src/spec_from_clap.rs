//! Convert a clap `Command` into a `SubcommandSpec` tree.

use clap::Command;
use hm_plugin_protocol::manifest::{ArgSpec, SubcommandSpec, ValueType};

/// Build a [`SubcommandSpec`] by introspecting a clap [`Command`].
///
/// Plugin authors can define their CLI schema with clap derive macros
/// and then call this in `hm_plugin!` to produce the manifest
/// automatically.
#[must_use]
pub fn spec_from_command(cmd: &Command) -> SubcommandSpec {
    let args: Vec<ArgSpec> = cmd
        .get_arguments()
        .filter(|a| {
            let id = a.get_id().as_str();
            id != "help" && id != "version"
        })
        .map(arg_spec_from_clap)
        .collect();

    let subcommands: Vec<SubcommandSpec> = cmd
        .get_subcommands()
        .filter(|c| c.get_name() != "help")
        .map(spec_from_command)
        .collect();

    SubcommandSpec {
        verb: cmd.get_name().to_string(),
        about: cmd
            .get_about()
            .map_or_else(String::new, |s| s.to_string()),
        args,
        subcommands,
    }
}

fn arg_spec_from_clap(arg: &clap::Arg) -> ArgSpec {
    let is_flag = matches!(
        arg.get_action(),
        clap::ArgAction::SetTrue | clap::ArgAction::Count
    );
    let is_positional = arg.get_long().is_none() && arg.get_short().is_none();

    if is_flag {
        ArgSpec::Flag {
            long: arg
                .get_long()
                .unwrap_or(arg.get_id().as_str())
                .to_string(),
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
            long: arg
                .get_long()
                .unwrap_or(arg.get_id().as_str())
                .to_string(),
            short: arg.get_short(),
            help: arg.get_help().map(|s| s.to_string()),
            required: arg.is_required_set(),
            value_type: ValueType::String,
            default: arg
                .get_default_values()
                .first()
                .and_then(|v| v.to_str())
                .map(String::from),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser, Subcommand};

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

        let positional = &spec.subcommands[0].args[0];
        assert!(matches!(positional, ArgSpec::Positional { name, required: true, .. } if name == "name"));

        let flag = &spec.subcommands[0].args[1];
        assert!(matches!(flag, ArgSpec::Flag { long, .. } if long == "dry-run"));
    }

    #[derive(Debug, Parser)]
    #[command(name = "opts", about = "Options test")]
    struct OptsCli {
        #[command(subcommand)]
        command: OptsCommand,
    }

    #[derive(Debug, Subcommand)]
    enum OptsCommand {
        /// List items.
        List {
            /// Max items.
            #[arg(long, default_value = "50")]
            limit: u32,
            /// Filter pattern.
            #[arg(short, long)]
            filter: Option<String>,
        },
    }

    #[test]
    fn handles_options_with_defaults() {
        use clap::CommandFactory;
        let cmd = OptsCli::command();
        let spec = spec_from_command(&cmd);
        let list = &spec.subcommands[0];
        assert_eq!(list.verb, "list");
        assert_eq!(list.args.len(), 2);

        let limit = &list.args[0];
        assert!(matches!(
            limit,
            ArgSpec::Option { long, default: Some(d), .. } if long == "limit" && d == "50"
        ));
    }
}
