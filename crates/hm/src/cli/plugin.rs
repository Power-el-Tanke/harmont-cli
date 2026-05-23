use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum PluginCommand {
    /// List installed plugins (embedded + user + project).
    List,

    /// Show one plugin's manifest in detail.
    Info {
        /// Plugin name (matches `name` field of the manifest).
        name: String,
    },

    /// Install a plugin from a file path or HTTPS URL.
    ///
    /// HTTPS URLs require `--pin <sha256>` for integrity.
    Install {
        /// Plugin source: local path (`./foo.wasm`) or HTTPS URL.
        source: String,

        /// SHA-256 hex digest to verify against. Required for HTTPS
        /// sources; optional for local paths.
        #[arg(long, value_name = "SHA256_HEX")]
        pin: Option<String>,
    },

    /// Remove an installed plugin by name.
    Remove {
        /// Plugin name.
        name: String,
    },
}
