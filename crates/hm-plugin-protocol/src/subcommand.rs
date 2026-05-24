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
