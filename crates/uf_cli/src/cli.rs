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
    /// Build the project for production.
    ///
    /// Runs Vite through `@uniflowed/vite`, with every module transformed by
    /// `uf transform`, then writes the build manifest and enforces
    /// `build.budgets`.
    Build {
        /// Print the emitted bundle's size, by chunk.
        #[arg(long)]
        size_report: bool,
    },
    /// Lint the project, then type check it with Flow.
    ///
    /// `uf lint` answers whether the source is well formed and idiomatic; this
    /// answers that and whether the types hold. A file that opts out with
    /// `@noflow` is parsed but not inferred.
    Check {
        /// Emit machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
        /// Only check files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Print a shell completion script.
    ///
    /// The script asks `uf` what may follow what, so a task added to
    /// `uf.config.js` completes immediately with nothing to regenerate.
    Completion {
        /// The shell to generate for.
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Answer a completion request. Not a command a person runs.
    #[command(hide = true, name = "__complete")]
    Complete {
        /// The words typed after `uf`, with the one being completed last.
        #[arg(trailing_var_arg = true)]
        words: Vec<String>,
    },
    /// Scaffold a new application or library.
    Create {
        #[command(subcommand)]
        command: CreateCommand,
    },
    /// Start the development server, with hot module replacement.
    Dev {
        /// Bind a routable address instead of loopback. Requires a non-empty
        /// `dev.allowedHosts` in `uf.config.js`; see `docs/security.md`.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        /// Listen on this port instead of `dev.port`.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
    },
    /// Generate API documentation from exported Flow source.
    ///
    /// Parses project-owned JavaScript with Meta's Flow parser, extracts
    /// JSDoc blocks attached to exported declarations, and writes Markdown.
    Doc {
        /// Directory to receive `api.md`.
        #[arg(
            long = "out",
            alias = "out-dir",
            value_name = "DIR",
            default_value = "docs/api"
        )]
        out_dir: Utf8PathBuf,
        /// Emit the report as JSON instead of writing Markdown.
        #[arg(long)]
        json: bool,
    },
    /// Inspect and switch the Capability JS Host uf runs JavaScript on.
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },
    /// Run a package's binary without installing it. Also `ufx`.
    Exec {
        /// The package to fetch and run, for example `@uniflowed/create`.
        package: String,
        /// Everything after the package name, handed to it untouched.
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Say what a command will do, and which provider does each part.
    ///
    /// `uf inspect` prints the resolved configuration; this answers the
    /// question someone asks when a command surprises them.
    Explain {
        /// The command to describe: dev, build, test, fmt, lint or check.
        command: String,
        #[arg(long)]
        json: bool,
    },
    /// Format every file in the project.
    ///
    /// Flow source is printed from the official Flow parser's syntax tree.
    Fmt {
        /// Report what would change and exit non-zero, writing nothing.
        #[arg(long)]
        check: bool,
        /// Only format files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Print the toolchain's version, host, and resolved paths.
    Info,
    /// Print the resolved configuration, after defaults and plugins.
    Inspect {
        /// Emit machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
    },
    /// Install the project's dependencies. Also `uf i`.
    #[command(visible_alias = "i")]
    Install,
    /// Serve uf's module transform over stdin/stdout, for the Vite plugin.
    ///
    /// Not a command a person runs: `@uniflowed/vite` spawns it once per build
    /// and pipes every module through it, so one process does the work a
    /// per-file `uf` invocation would have paid startup for thousands of times.
    #[command(hide = true)]
    Transform,
    /// Lint the project without type checking it.
    Lint {
        /// Emit machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
        /// Only lint files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Serve the language server over stdin/stdout, for an editor.
    Lsp,
    /// Run the checks and code generation a commit should not go without.
    Prepare,
    /// Publish the project's packages to the registry.
    Publish,
    /// Cut a release: calculate the next version and write its metadata.
    Release {
        /// How far to move the version.
        #[arg(value_enum)]
        bump: ReleaseBump,
    },
    /// Run a task from `uf.config.js`, or list them. Also `ufr`.
    Run {
        /// The task to run, as named under `tasks` in `uf.config.js`.
        /// Omit it to see what this project defines.
        script: Option<String>,
        /// Everything after the task name, handed to it untouched.
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run the project's tests.
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
        /// Rewrite any snapshot that did not match.
        ///
        /// Off by default, and deliberately: a snapshot that updates itself
        /// whenever the code changes is not a test, it is a record of what the
        /// code did.
        #[arg(short = 'u', long)]
        update_snapshots: bool,
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
    /// Upgrade the project's dependencies and the toolchain.
    Upgrade,
    /// Switch the active uf toolchain, for example `uf use uf@0.1.0`.
    Use {
        /// The toolchain to activate.
        runtime: String,
    },
}

impl Commands {
    /// Whether this invocation must emit pure JSON on stdout.
    pub(crate) fn wants_json(&self) -> bool {
        matches!(
            self,
            Self::Check { json: true, .. }
                | Self::Doc { json: true, .. }
                | Self::Explain { json: true, .. }
                | Self::Inspect { json: true }
                | Self::Lint { json: true, .. }
                | Self::Test { json: true, .. }
        )
    }

    /// Whether this invocation hands stdout to a protocol or a child process
    /// rather than to a reader, in which case nothing may be rendered onto it.
    ///
    /// `uf run` with no task name is the exception in its own command: it runs
    /// nothing and lists what it could have run, so stdout is a reader's again.
    pub(crate) fn owns_stdout(&self) -> bool {
        match self {
            // `uf completion` is piped into `eval` and `uf __complete` into a
            // completion list; a banner on either is a syntax error in
            // somebody's shell.
            Self::Complete { .. } | Self::Completion { .. } | Self::Lsp | Self::Transform => true,
            Self::Run { script, .. } => script.is_some(),
            _ => false,
        }
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

/// A shell uf can generate completion for.
// `PowerShell` ends in the enum's own name, which clippy reads as a stutter.
// It is the shell's name, and renaming it would be worse than the lint.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum Shell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    #[value(name = "powershell")]
    PowerShell,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum AppTemplate {
    React,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ReleaseBump {
    Alpha,
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EnvCommand {
    /// Report which tools are installed on this machine.
    Doctor,
    /// Set the active `.env` profile.
    Use {
        /// The profile name, e.g. `production`.
        name: String,
    },
    /// Install the runtimes and package managers `uf.config.js` declares.
    ///
    /// Into a store shared by every repository on this machine, linked into
    /// this one. Nothing is installed globally and `PATH` is not changed.
    Install,
    /// List what this project declares and what the store holds.
    List,
    /// Run a command with this project's toolchain in front of `PATH`.
    Exec {
        /// The command and its arguments.
        #[arg(trailing_var_arg = true, required = true)]
        command: Vec<String>,
    },
    /// Delete store entries no repository is using.
    Gc {
        /// Say what would go, and remove nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_commands_are_the_only_ones_that_suppress_rendering() {
        assert!(Commands::Inspect { json: true }.wants_json());
        assert!(!Commands::Inspect { json: false }.wants_json());
        assert!(
            Commands::Lint {
                json: true,
                paths: Vec::new()
            }
            .wants_json()
        );
        assert!(
            Commands::Check {
                json: true,
                paths: Vec::new()
            }
            .wants_json()
        );
        assert!(!Commands::Build { size_report: false }.wants_json());
    }

    #[test]
    fn protocol_commands_own_stdout() {
        assert!(Commands::Lsp.owns_stdout());
        assert!(
            Commands::Run {
                script: Some("build".to_string()),
                args: Vec::new(),
            }
            .owns_stdout()
        );
        assert!(
            !Commands::Run {
                script: None,
                args: Vec::new(),
            }
            .owns_stdout(),
            "listing tasks renders, so it must keep stdout"
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
