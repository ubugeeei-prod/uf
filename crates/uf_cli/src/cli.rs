//! The clap definitions: every subcommand, flag, and value enum.

use camino::Utf8PathBuf;
use clap::{Subcommand, ValueEnum};
use uf_term::ColorChoice;

/// The `--color` flag, mapped to [`ColorChoice`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub(crate) enum ColorOption {
    /// Colour when stdout is a terminal that supports it.
    #[default]
    Auto,
    /// Always colour, even when redirected.
    Always,
    /// Never colour.
    Never,
}

impl From<ColorOption> for ColorChoice {
    fn from(value: ColorOption) -> Self {
        match value {
            ColorOption::Auto => Self::Auto,
            ColorOption::Always => Self::Always,
            ColorOption::Never => Self::Never,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    Build {
        #[arg(long)]
        size_report: bool,
    },
    Check {
        #[arg(long)]
        json: bool,
    },
    Create {
        #[command(subcommand)]
        command: CreateCommand,
    },
    Dev {
        /// Bind a routable address instead of loopback. Requires a non-empty
        /// `dev.allowedHosts` in `uf.config.js`; see `docs/security.md`.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        #[arg(long, hide = true)]
        once: bool,
    },
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    Exec {
        package: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Fmt {
        #[arg(long)]
        check: bool,
    },
    Info,
    Inspect {
        #[arg(long)]
        json: bool,
    },
    Install,
    /// Serve uf's module transform over stdin/stdout, for the Vite plugin.
    ///
    /// Not a command a person runs: `@uniflowed/vite` spawns it once per build
    /// and pipes every module through it, so one process does the work a
    /// per-file `uf` invocation would have paid startup for thousands of times.
    #[command(hide = true)]
    Transform,
    Lint {
        #[arg(long)]
        json: bool,
    },
    Lsp,
    Prepare,
    Publish,
    Release {
        #[arg(value_enum)]
        bump: ReleaseBump,
    },
    Run {
        script: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Test {
        /// List what would run instead of running it.
        #[arg(long)]
        list: bool,
        /// Re-run the affected tests whenever a source file changes.
        #[arg(long)]
        watch: bool,
        /// Emit machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
        /// Keep only tests whose fully qualified name contains PATTERN.
        #[arg(short = 't', long, value_name = "PATTERN")]
        filter: Option<String>,
        /// Stop once N tests have failed; N defaults to 1.
        #[arg(long, value_name = "N", num_args = 0..=1, default_missing_value = "1")]
        bail: Option<usize>,
        /// Re-run a failing test up to N more times.
        #[arg(long, value_name = "N", default_value_t = 0)]
        retry: u32,
        /// Run at most N files at once; defaults to one per core.
        #[arg(short = 'j', long, value_name = "N")]
        threads: Option<usize>,
        /// How often `--watch` looks for changes, in milliseconds.
        #[arg(long, value_name = "MS")]
        watch_interval: Option<u64>,
        /// Only run files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    Use {
        runtime: String,
    },
    Upgrade,
}

impl Commands {
    /// Whether this invocation must emit pure JSON on stdout.
    pub(crate) fn wants_json(&self) -> bool {
        matches!(
            self,
            Self::Check { json: true }
                | Self::Inspect { json: true }
                | Self::Lint { json: true }
                | Self::Test { json: true, .. }
        )
    }

    /// Whether this invocation hands stdout to a protocol or a child process
    /// rather than to a reader, in which case nothing may be rendered onto it.
    pub(crate) fn owns_stdout(&self) -> bool {
        matches!(self, Self::Lsp | Self::Run { .. } | Self::Transform)
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum CreateCommand {
    App {
        #[arg(value_enum, default_value = "react")]
        template: AppTemplate,
        path: Option<Utf8PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
    Lib {
        path: Option<Utf8PathBuf>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum AppTemplate {
    React,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ReleaseBump {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EnvCommand {
    Doctor,
    Use { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_commands_are_the_only_ones_that_suppress_rendering() {
        assert!(Commands::Inspect { json: true }.wants_json());
        assert!(!Commands::Inspect { json: false }.wants_json());
        assert!(Commands::Lint { json: true }.wants_json());
        assert!(Commands::Check { json: true }.wants_json());
        assert!(!Commands::Build { size_report: false }.wants_json());
    }

    #[test]
    fn protocol_commands_own_stdout() {
        assert!(Commands::Lsp.owns_stdout());
        assert!(
            Commands::Run {
                script: "build".to_string(),
                args: Vec::new(),
            }
            .owns_stdout()
        );
        assert!(!Commands::Build { size_report: false }.owns_stdout());
    }

    #[test]
    fn the_color_flag_maps_onto_the_terminal_choice() {
        assert_eq!(ColorChoice::from(ColorOption::Auto), ColorChoice::Auto);
        assert_eq!(ColorChoice::from(ColorOption::Always), ColorChoice::Always);
        assert_eq!(ColorChoice::from(ColorOption::Never), ColorChoice::Never);
        assert_eq!(ColorOption::default(), ColorOption::Auto);
    }
}
