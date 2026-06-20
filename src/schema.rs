//! The clispec v0.2 contract emitted by `downstat schema`.
//!
//! Conforms to <https://clispec.dev/schema/v0.2.json> (validated by a test
//! against the vendored copy in `schemas/clispec-v0.2.json`).

use serde_json::{Value, json};

/// The version of The CLI Spec this document conforms to.
pub const CLISPEC_VERSION: &str = "0.2";

/// Build the clispec contract as a JSON value.
pub fn contract() -> Value {
    json!({
        "clispec": CLISPEC_VERSION,
        "name": "downstat",
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "global_args": [
            {
                "name": "--output",
                "type": "string",
                "enum": ["auto", "json", "text"],
                "default": "auto",
                "description": "Output format. auto = text on a TTY, JSON when piped."
            },
            {
                "name": "--only",
                "type": "string",
                "enum": ["crates", "pypi", "npm", "github"],
                "description": "Restrict to one or more registries (repeatable)."
            },
            {
                "name": "--all",
                "type": "boolean",
                "default": false,
                "description": "Query every package listed in ./downstat.toml."
            }
        ],
        "commands": [
            {
                "name": "report",
                "description": "Show downloads + latest version for one or more package names across crates.io, PyPI, npm and GitHub releases. The default command, invoked as `downstat <name>...`.",
                "mutating": false,
                "stability": "stable",
                "args": [
                    {"name": "names", "type": "string[]", "required": false, "description": "Package names to look up (omit with --all)."}
                ],
                "output_fields": [
                    {"name": "name", "type": "string", "description": "The package name queried."},
                    {"name": "registries", "type": "array", "description": "Per-registry stats: {registry, found, version, downloads:{total,recent,window}, url, note}."}
                ]
            },
            {
                "name": "schema",
                "description": "Print this clispec contract as JSON.",
                "mutating": false,
                "stability": "stable"
            },
            {
                "name": "completions",
                "description": "Generate a shell completion script.",
                "mutating": false,
                "stability": "stable",
                "args": [
                    {"name": "shell", "type": "string", "required": true, "enum": ["bash", "zsh", "fish", "powershell", "elvish"], "description": "Target shell."}
                ]
            }
        ],
        "errors": [
            {"kind": "usage", "exit_code": 3, "retryable": false, "description": "Invalid command-line arguments or config."},
            {"kind": "no_data", "exit_code": 1, "retryable": false, "description": "A queried name was found on no registry."},
            {"kind": "http", "exit_code": 2, "retryable": true, "description": "A registry request failed at the network level."},
            {"kind": "parse", "exit_code": 2, "retryable": false, "description": "A registry returned an unparseable response."}
        ],
        "notes": "Download counts are normalized but not directly comparable across registries (different windows): crates.io reports total + 90-day, npm/PyPI report a 30-day window, GitHub reports summed release-asset totals. PyPI counts come from pypistats.org. ghcr and Homebrew taps expose no public download counts and are omitted."
    })
}

/// The contract as a pretty-printed JSON string.
pub fn contract_json() -> String {
    serde_json::to_string_pretty(&contract()).expect("contract serializes")
}
