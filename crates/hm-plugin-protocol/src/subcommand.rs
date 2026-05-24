//! Wire type for subcommand invocations.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use schemars::JsonSchema as DeriveJsonSchema;
use serde::{Deserialize, Serialize};

use crate::Value;

/// Carried into the plugin's subcommand entry point. The host has
/// already parsed argv on the plugin's behalf using the schema the
/// plugin declared in its manifest.
#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize, Serialize, Deserialize, DeriveJsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SubcommandInput {
    /// Verb path: `["cloud", "org", "switch"]` for `hm cloud org switch`.
    pub verb_path: Vec<String>,
    /// Positional + option args, already parsed and JSON-encoded.
    pub args: Value,
    /// `HARMONT_*` env vars + any vars the plugin declared interest in.
    pub env: BTreeMap<String, String>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn subcommand_input_borsh_round_trip() {
        let input = SubcommandInput {
            verb_path: vec!["cloud".into(), "login".into()],
            args: Value::Object({
                let mut m = BTreeMap::new();
                m.insert("org".into(), Value::Str("mesa".into()));
                m.insert("force".into(), Value::Bool(true));
                m
            }),
            env: {
                let mut e = BTreeMap::new();
                e.insert("HARMONT_TOKEN".into(), "abc123".into());
                e
            },
        };
        let bytes = borsh::to_vec(&input).unwrap();
        let decoded = SubcommandInput::try_from_slice(&bytes).unwrap();
        assert_eq!(input, decoded);
    }
}
