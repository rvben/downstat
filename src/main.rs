//! downstat CLI: downloads + latest version across registries.
//!
//! Follows The CLI Spec (clispec.dev): text on a TTY, JSON when piped,
//! structured error envelopes on the last line of stderr, a `schema`
//! subcommand, and read-only behavior (every command is `mutating: false`).

use std::io::{IsTerminal, Write};
use std::process::ExitCode;

use clap::error::ErrorKind as ClapErrorKind;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use downstat::{DownstatError, OutputFormat, Registry, Request, ReqwestHttp, run, schema};
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "downstat",
    version,
    about = "Downloads + latest version for your packages across crates.io, PyPI, npm and GitHub releases.",
    long_about = "Downloads + latest version for your packages across crates.io, PyPI, npm and GitHub releases, in one view.\n\n\
                  `downstat <name>...` looks up specific packages; `downstat --all` reads ./downstat.toml.\n\n\
                  Run `downstat schema` for the machine-readable contract (clispec.dev).",
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Package names to look up (omit with --all).
    #[arg(value_name = "NAME")]
    names: Vec<String>,

    /// Query every package listed in ./downstat.toml.
    #[arg(long, global = true)]
    all: bool,

    /// Restrict to a registry (repeatable): crates, pypi, npm, github.
    #[arg(long, value_enum, global = true)]
    only: Vec<CliRegistry>,

    /// Output format; auto = text on a TTY, JSON when piped.
    #[arg(long, short = 'o', value_enum, default_value = "auto", global = true)]
    output: CliOutput,
}

#[derive(Subcommand)]
enum Command {
    /// Print the machine-readable contract (clispec.dev) as JSON.
    Schema,
    /// Generate a shell completion script.
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum CliOutput {
    Auto,
    Json,
    Text,
}

#[derive(Clone, Copy, ValueEnum)]
enum CliRegistry {
    Crates,
    Pypi,
    Npm,
    Github,
}

impl From<CliRegistry> for Registry {
    fn from(r: CliRegistry) -> Self {
        match r {
            CliRegistry::Crates => Registry::Crates,
            CliRegistry::Pypi => Registry::Pypi,
            CliRegistry::Npm => Registry::Npm,
            CliRegistry::Github => Registry::Github,
        }
    }
}

impl CliOutput {
    fn resolve(self) -> OutputFormat {
        match self {
            CliOutput::Json => OutputFormat::Json,
            CliOutput::Text => OutputFormat::Table,
            CliOutput::Auto => {
                if std::io::stdout().is_terminal() {
                    OutputFormat::Table
                } else {
                    OutputFormat::Json
                }
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct Config {
    #[serde(default)]
    packages: Vec<String>,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return handle_clap_error(e),
    };

    match &cli.command {
        Some(Command::Schema) => {
            println!("{}", schema::contract_json());
            return ExitCode::SUCCESS;
        }
        Some(Command::Completions { shell }) => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(*shell, &mut cmd, name, &mut std::io::stdout());
            return ExitCode::SUCCESS;
        }
        None => {}
    }

    match report(&cli) {
        Ok(output) => {
            let _ = writeln!(std::io::stdout(), "{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            emit_error(&err);
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

fn report(cli: &Cli) -> Result<String, DownstatError> {
    let names = resolve_names(cli)?;
    let only = if cli.only.is_empty() {
        None
    } else {
        Some(cli.only.iter().copied().map(Registry::from).collect())
    };
    let request = Request {
        names,
        only,
        format: cli.output.resolve(),
    };
    let http = ReqwestHttp::new()?;
    run(&http, &request)
}

fn resolve_names(cli: &Cli) -> Result<Vec<String>, DownstatError> {
    if cli.all {
        let text = std::fs::read_to_string("downstat.toml").map_err(|e| DownstatError::Usage {
            message: format!("--all needs ./downstat.toml: {e}"),
        })?;
        let config: Config = toml::from_str(&text).map_err(|e| DownstatError::Usage {
            message: format!("invalid downstat.toml: {e}"),
        })?;
        if config.packages.is_empty() {
            return Err(DownstatError::Usage {
                message: "downstat.toml has no `packages`".into(),
            });
        }
        Ok(config.packages)
    } else if cli.names.is_empty() {
        Err(DownstatError::Usage {
            message: "no package given (try `downstat <name>` or `downstat --all`)".into(),
        })
    } else {
        Ok(cli.names.clone())
    }
}

/// Help and version print normally and exit 0; every other clap failure becomes
/// a structured `usage` error envelope.
fn handle_clap_error(e: clap::Error) -> ExitCode {
    match e.kind() {
        ClapErrorKind::DisplayHelp
        | ClapErrorKind::DisplayVersion
        | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            let _ = e.print();
            ExitCode::SUCCESS
        }
        _ => {
            let err = DownstatError::Usage {
                message: e.to_string().trim().to_string(),
            };
            emit_error(&err);
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Write the clispec error envelope as the last line of stderr.
fn emit_error(err: &DownstatError) {
    let mut error = serde_json::Map::new();
    error.insert("kind".into(), json!(err.kind()));
    error.insert("message".into(), json!(err.to_string()));
    error.insert("exit_code".into(), json!(err.exit_code()));
    error.insert("retryable".into(), json!(err.retryable()));
    if let Some(hint) = err.hint() {
        error.insert("hint".into(), json!(hint));
    }
    eprintln!("{}", json!({ "error": error }));
}
