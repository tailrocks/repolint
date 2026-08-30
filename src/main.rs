mod config;
mod map;
mod report;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use config::Config;
use map::MapDocument;
use report::{Finding, OutputFormat, Report, Severity};
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(
    name = "repolint",
    version,
    about = "Generate and gate a repository map"
)]
struct Cli {
    #[arg(long, global = true, default_value = ".", value_name = "PATH")]
    root: PathBuf,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Gate the repository map without changing files.
    Check(CheckArgs),
    /// Generate the README repository map.
    Map(MapArgs),
}

#[derive(Debug, Args)]
struct CheckArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(long)]
    error_on_warn: bool,
}

#[derive(Debug, Args)]
struct MapArgs {
    /// Write the generated block into README.md.
    #[arg(long)]
    write: bool,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("configuration error: {0}")]
    Config(#[from] config::ConfigError),
    #[error("map error: {0}")]
    Map(#[from] map::MapError),
    #[error("output error: {0}")]
    Output(#[from] serde_json::Error),
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("repolint: {error}");
            2
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<i32, AppError> {
    let config = Config::load(&cli.root)?;

    match cli.command.unwrap_or(Command::Check(CheckArgs {
        format: OutputFormat::Human,
        error_on_warn: false,
    })) {
        Command::Check(args) => run_check(&cli.root, &config, args),
        Command::Map(args) => run_map(&cli.root, &config, args),
    }
}

fn run_check(root: &std::path::Path, config: &Config, args: CheckArgs) -> Result<i32, AppError> {
    let mut report = Report::default();

    if config.repo.is_none() {
        report.push(Finding::new(
            "map.gate",
            Severity::Warn,
            "repository is not adopted; map checks are warn-only until repolint.toml contains [repo]",
            None,
        ));
    } else {
        report.extend(map::check(root, config)?);
    }

    report.write(args.format)?;
    if report.has_error() || (args.error_on_warn && report.has_warning()) {
        Ok(1)
    } else {
        Ok(0)
    }
}

fn run_map(root: &std::path::Path, config: &Config, args: MapArgs) -> Result<i32, AppError> {
    let document = MapDocument::build(root, config)?;
    if args.write {
        document.write(root)?;
    } else {
        print!("{}", document.block());
    }
    Ok(0)
}
