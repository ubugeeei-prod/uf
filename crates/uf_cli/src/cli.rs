//! The clap definitions: every subcommand, flag, and value enum.

use camino::Utf8PathBuf;
use clap::{Subcommand, ValueEnum};
use uf_config::DeployAdapter;
use uf_pm::DependencyKind;
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

/// The `--adapter` flag, mapped to [`DeployAdapter`].
///
/// A value enum of its own rather than `DeployAdapter` directly: clap's
/// `ValueEnum` is a derive on the type, and `uf_config` is data that the LSP,
/// the docs build and the plugin host all read — putting a CLI dependency in
/// it to spell one flag would be the wrong crate paying for it.
///
/// Every target is accepted here, including the six nobody has written. That
/// is deliberate: `uf build --adapter edge` should be answered with a sentence
/// saying so and naming the issue, not with clap's list of valid values, which
/// would leave a reader unable to tell "uf will never have this" from "uf does
/// not have this yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum DeployAdapterOption {
    /// A directory served by `node:http`.
    Node,
    /// Bun.
    Bun,
    /// Deno.
    Deno,
    /// A Web-standard worker.
    Edge,
    /// A serverless function.
    Serverless,
    /// Prerendered files only.
    Static,
    /// A container image.
    Container,
}

impl From<DeployAdapterOption> for DeployAdapter {
    fn from(value: DeployAdapterOption) -> Self {
        match value {
            DeployAdapterOption::Node => Self::Node,
            DeployAdapterOption::Bun => Self::Bun,
            DeployAdapterOption::Deno => Self::Deno,
            DeployAdapterOption::Edge => Self::Edge,
            DeployAdapterOption::Serverless => Self::Serverless,
            DeployAdapterOption::Static => Self::Static,
            DeployAdapterOption::Container => Self::Container,
        }
    }
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

/// Which `package.json` field `uf add` writes a package into.
///
/// A flag apiece rather than `--save <kind>`: `--dev` is what every manager
/// this delegates to spells it, and a person who types `uf add --dev` has
/// already typed it four other ways this week.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AddTarget {
    /// `devDependencies`.
    pub(crate) dev: bool,
    /// `optionalDependencies`.
    pub(crate) optional: bool,
    /// `peerDependencies`.
    pub(crate) peer: bool,
}

impl From<AddTarget> for DependencyKind {
    fn from(target: AddTarget) -> Self {
        // clap has already refused any two of them together, so the order here
        // decides nothing; production is what remains when none was asked for.
        match target {
            AddTarget { dev: true, .. } => Self::Dev,
            AddTarget { optional: true, .. } => Self::Optional,
            AddTarget { peer: true, .. } => Self::Peer,
            _ => Self::Prod,
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Add dependencies with the project's own package manager.
    ///
    /// `uf add react react-dom@^19` resolves, installs, and writes both the
    /// manifest and the lockfile, with lifecycle scripts refused unless
    /// `pm.allowLifecycleScripts` says otherwise. A specifier is passed to the
    /// manager exactly as written, so a range, a tag, an alias or a path all
    /// mean what they mean there.
    Add {
        /// Record them in `devDependencies`.
        #[arg(long, conflicts_with_all = ["optional", "peer"])]
        dev: bool,
        /// Record them in `optionalDependencies`.
        #[arg(long, conflicts_with = "peer")]
        optional: bool,
        /// Record them in `peerDependencies`.
        #[arg(long)]
        peer: bool,
        /// The packages: `react`, `react@^19`, `./packages/ui`.
        #[arg(value_name = "SPEC", required = true)]
        specs: Vec<String>,
    },
    /// Build the project for production.
    ///
    /// Runs Vite through `@uniflowed/vite`, with every module transformed by
    /// `uf transform`, then writes the build manifest and enforces
    /// `build.budgets`.
    Build {
        /// Print the emitted bundle's size, by chunk.
        #[arg(long)]
        size_report: bool,
        /// Also write the application as one executable file, next to the
        /// build. Needs Bun on PATH; the file itself needs nothing.
        #[arg(long)]
        compile: bool,
        /// Also write a directory that can be copied to a host with a
        /// JavaScript runtime and nothing else. Overrides
        /// `app.runtime.deploy.adapter`.
        #[arg(long, value_name = "TARGET")]
        adapter: Option<DeployAdapterOption>,
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
    /// Run a package's binary, fetching it only with `--yes`. Also `ufx`.
    Exec {
        /// Fetch a package the project has not installed, and run it.
        ///
        /// Off by default: fetching an unpinned name and executing it is the
        /// most dangerous thing a package manager does, and uf already refuses
        /// to run a dependency's install scripts without being asked.
        #[arg(long, short = 'y')]
        yes: bool,
        /// The package to run, for example `@uniflowed/create`.
        package: String,
        /// Everything after the package name, handed to it untouched.
        ///
        /// `allow_hyphen_values` because "untouched" has to include `--fix`:
        /// without it `ufx eslint --fix` was `error: unexpected argument
        /// '--fix' found`, which is uf reading an argument that was never
        /// addressed to it.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
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
    Install {
        /// Install exactly what the lockfile pins, and fail when it is stale.
        ///
        /// What CI runs: `npm ci`, `pnpm install --frozen-lockfile`, `yarn
        /// install --immutable`, `bun install --frozen-lockfile`. A lockfile
        /// that has drifted from the manifests is the failure, rather than
        /// being quietly resolved away.
        #[arg(long)]
        frozen_lockfile: bool,
    },
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
    /// Serve the production build through Vite's preview server.
    ///
    /// The step between `uf build` and deploying: the one place a person finds
    /// out that what worked in `uf dev` also works bundled, minified, hashed
    /// and served from `dist/`. Route handlers and routes that were never
    /// prerendered are served too, so it is the build rather than half of it.
    Preview {
        /// Bind a routable address instead of loopback. Requires a non-empty
        /// `dev.allowedHosts` in `uf.config.js`; see `docs/security.md`.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        /// Listen on this port instead of 4173.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
    },
    /// Publish the project's packages to the registry.
    Publish,
    /// Cut a release: calculate the next version and write its metadata.
    Release {
        /// How far to move the version.
        #[arg(value_enum)]
        bump: ReleaseBump,
    },
    /// Remove dependencies with the project's own package manager.
    ///
    /// Takes them out of every dependency field that lists them, out of the
    /// lockfile, and out of `node_modules`. A name the manifest never listed is
    /// not an error: the project ends up the way it was asked to be either way.
    Remove {
        /// The packages, by name.
        #[arg(value_name = "NAME", required = true)]
        names: Vec<String>,
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
    /// Serve the production build, with no bundler in the process.
    ///
    /// What a deployment runs. `uf preview` checks a build through Vite;
    /// this serves the same output through uf's own server, so a host needs
    /// neither Vite nor a network to answer a request.
    Start {
        /// Bind this address instead of every interface. Also read from
        /// `HOST`.
        #[arg(long, value_name = "HOST")]
        host: Option<String>,
        /// Listen on this port instead of 3000. Also read from `PORT`.
        #[arg(long, value_name = "PORT")]
        port: Option<u16>,
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
    /// Update dependencies to the newest version their range allows.
    ///
    /// The ranges in `package.json` are not touched; the lockfile is. Name
    /// packages to hold the rest still, or name none to update everything.
    Update {
        /// The packages to update; all of them when none is named.
        #[arg(value_name = "PACKAGE")]
        packages: Vec<String>,
    },
    /// Re-read the workspace and record the package and runtime plan.
    ///
    /// It fetches nothing and it does not replace the uf binary, in spite of
    /// its name: `uf update` moves your dependencies and `uf use` moves the
    /// toolchain. See ubugeeei-prod/uf#287.
    Upgrade,
    /// Switch the active uf toolchain, for example `uf use uf@0.1.0`.
    Use {
        /// The toolchain to activate.
        runtime: String,
    },
    /// Explain why a package is in the dependency tree.
    ///
    /// Answered by the manager that resolved it, from its own lockfile, so the
    /// chain it prints is the chain that was actually installed.
    Why {
        /// The package to explain.
        #[arg(value_name = "NAME")]
        package: String,
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
        /// The template, or the path when it is the only argument given.
        ///
        /// Two positionals with the template first is what `uf create app`
        /// has always taken, and it is the right grammar once there is more
        /// than one template to choose from. It is the wrong grammar for the
        /// first command a reader types, which is one word: `uf create app
        /// my-site` is what the home page and the CLI reference both print,
        /// and it was rejected with "invalid value 'my-site' for
        /// '[TEMPLATE]'". `ufx @uniflowed/create app my-site` already read a
        /// lone argument as the path, so the two front doors disagreed as
        /// well. See ubugeeei-prod/uf#322.
        ///
        /// A single argument that names a template is the template — so
        /// `uf create app react` still scaffolds into the current directory,
        /// and a directory that wants to be called `react` is written out in
        /// full as `uf create app react react`.
        #[arg(value_name = "TEMPLATE|PATH")]
        template_or_path: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum AppTemplate {
    React,
}

impl AppTemplate {
    /// Every template, for a message that has to list them.
    pub(crate) const ALL: [&'static str; 1] = ["react"];

    /// The template `value` names, or `None` when it names something else.
    ///
    /// The one place the spelling of a template is decided, so
    /// `uf create app`, `ufx @uniflowed/create app` and the error that lists
    /// them cannot drift apart — they did, and #322 is what that cost.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "react" => Some(Self::React),
            _ => None,
        }
    }
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
        assert!(
            !Commands::Build {
                size_report: false,
                compile: false,
                adapter: None
            }
            .wants_json()
        );
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
        assert!(
            !Commands::Build {
                size_report: false,
                compile: false,
                adapter: None
            }
            .owns_stdout()
        );
    }

    #[test]
    fn the_color_flag_maps_onto_the_terminal_choice() {
        assert_eq!(ColorChoice::from(ColorOption::Auto), ColorChoice::Auto);
        assert_eq!(ColorChoice::from(ColorOption::Always), ColorChoice::Always);
        assert_eq!(ColorChoice::from(ColorOption::Never), ColorChoice::Never);
        assert_eq!(ColorOption::default(), ColorOption::Auto);
    }
}
