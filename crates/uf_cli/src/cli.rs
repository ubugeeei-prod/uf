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
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
        /// Also write the application as one executable file, next to the
        /// build. Produced on the project's Capability JS Host — Bun or Node
        /// 25.5 and newer; the file itself needs nothing.
        #[arg(long)]
        compile: bool,
        /// Compile for this platform rather than for this machine, as a
        /// target triple: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`.
        ///
        /// Cross-compiling downloads that platform's runtime, so the first
        /// build for a target needs the network. Bun's backend only; Node
        /// single-executable applications cannot cross-compile.
        //
        // A free-form string rather than a `ValueEnum`, for the reason
        // `--adapter` is one: clap's list of accepted values cannot say *why*
        // a triple is not accepted, and "uf has never built for this" and
        // "your Bun is too old for this" are different sentences that a reader
        // has to be able to tell apart. `compile::parse_target` says both.
        #[arg(long, value_name = "TRIPLE", requires = "compile")]
        target: Option<String>,
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
        /// Rewrite the files, applying every fix that cannot change what the
        /// program does.
        ///
        /// A fix is applied only when the replacement is a synonym the
        /// language already treats as the original, so the program after it
        /// means what it meant before. What is written parses, and a file that
        /// passed `uf fmt --check` before still passes it after. Running this
        /// twice is running it once.
        #[arg(long)]
        fix: bool,
        /// Also apply the fixes whose correctness rests on something the rule
        /// could not check.
        ///
        /// "Unsafe" means the edit is the one the rule asks for and there is no
        /// second plausible spelling of it, but applying it can change what the
        /// program does: `export let count = 0` becomes `export const
        /// count = 0`, which throws where the old code reassigned it. The file
        /// still parses and still formats — read the diff before committing it.
        #[arg(long, conflicts_with = "fix")]
        fix_unsafe: bool,
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
    /// Scaffold a project into the current directory.
    ///
    /// `uf init` is the current directory and `uf new <path>` is a new one,
    /// which is the split `uf create` did not make: it took one optional
    /// positional that was a template when it named one and a directory when
    /// it did not, so `uf create app react` and `uf create app my-site` did
    /// two different things and neither spelling said which. See
    /// ubugeeei-prod/uf#322 for what that cost, and #488 for the rename.
    Init {
        /// The template to scaffold. `react` is the only one today.
        #[arg(value_name = "TEMPLATE")]
        template: Option<String>,
        /// Scaffold a library rather than an application.
        #[arg(long)]
        lib: bool,
        /// The package name, when it should not be the directory's.
        #[arg(long)]
        name: Option<String>,
        /// Write into a directory that already holds files.
        #[arg(long)]
        force: bool,
    },
    /// Scaffold a project into a new directory.
    New {
        /// The directory to create, relative to the current one. Its last
        /// segment is the package's name unless `--name` says otherwise.
        #[arg(value_name = "PATH")]
        path: Utf8PathBuf,
        /// The template to scaffold. `react` is the only one today.
        #[arg(value_name = "TEMPLATE")]
        template: Option<String>,
        /// Scaffold a library rather than an application.
        #[arg(long)]
        lib: bool,
        /// The package name, when it should not be the directory's.
        #[arg(long)]
        name: Option<String>,
        /// Write into a directory that already holds files.
        #[arg(long)]
        force: bool,
    },
    /// The older spelling of `uf init` and `uf new`.
    ///
    /// Hidden rather than removed: it is in every published document and in
    /// `ufx @uniflowed/create`, and an alpha that deletes the command its own
    /// home page prints teaches people to distrust the next release more than
    /// it teaches them the new name.
    #[command(hide = true)]
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
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
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
    /// The message catalogue: out to a translator, and back.
    ///
    /// `@uniflowed/i18n` declares each message beside its parameters in source.
    /// `uf i18n extract` turns that into one JSON file a translation vendor
    /// accepts, and `uf i18n merge` reads the translated file back into the
    /// locale module `defineLocales` loads.
    I18n {
        #[command(subcommand)]
        command: I18nCommand,
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
    /// Serve uf's image and font pipeline over stdin/stdout, for the Vite plugin.
    ///
    /// Not a command a person runs, and the same arrangement as `transform`
    /// for the same reason: decoding and re-encoding every image in a project
    /// is native work driven from a JavaScript plugin, and one long-lived
    /// process is what keeps it from paying start-up once per asset.
    #[command(hide = true)]
    Assets,
    /// Remove what a rebuild would write again.
    ///
    /// The build's output, uf's own per-project state under `.uf/`, and the
    /// caches. Not `node_modules` unless asked, and never a lockfile: the line
    /// is the network, and a lockfile is an input.
    Clean {
        /// Also remove `node_modules`, which costs a network round trip.
        #[arg(long)]
        deps: bool,
        /// Print what would be removed, and remove nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Lint the project without type checking it.
    Lint {
        /// Emit machine-readable JSON on stdout.
        #[arg(long)]
        json: bool,
        /// Rewrite the files, applying every fix that cannot change what the
        /// program does.
        ///
        /// A fix is applied only when the replacement is a synonym the
        /// language already treats as the original, so the program after it
        /// means what it meant before. What is written parses, and a file that
        /// passed `uf fmt --check` before still passes it after. Running this
        /// twice is running it once.
        #[arg(long)]
        fix: bool,
        /// Also apply the fixes whose correctness rests on something the rule
        /// could not check.
        ///
        /// "Unsafe" means the edit is the one the rule asks for and there is no
        /// second plausible spelling of it, but applying it can change what the
        /// program does: `export let count = 0` becomes `export const
        /// count = 0`, which throws where the old code reassigned it. The file
        /// still parses and still formats — read the diff before committing it.
        #[arg(long, conflicts_with = "fix")]
        fix_unsafe: bool,
        /// Only lint files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Serve the language server over stdin/stdout, for an editor.
    Lsp,
    /// Serve the Model Context Protocol over stdin/stdout, for an agent.
    ///
    /// The read-only commands become tools of the same name; the two that
    /// write are separate and say so: `uf_fmt_write`, `uf_lint_fix`.
    Mcp,
    /// Run the checks and code generation a commit should not go without.
    Prepare {
        /// Apply `uf lint`'s safe fixes to the staged files and format them,
        /// instead of only reporting what is wrong with them.
        ///
        /// Whatever it rewrites is left in the working tree unstaged, and the
        /// run fails so the commit stops: the fixes are yours to read and
        /// stage, not uf's to slip into a commit you already wrote a message
        /// for.
        #[arg(long)]
        fix: bool,
    },
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
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
    },
    /// Publish the project's packages to the registry.
    Publish,
    /// Cut a release: calculate the next version and write its metadata.
    Release {
        /// How far to move the version.
        #[arg(value_enum)]
        bump: ReleaseBump,
        /// Rewrite a changelog section that is already there.
        ///
        /// `uf release` refuses to overwrite a section for the version it
        /// planned, because a binary older than the tree plans a version the
        /// tree has already published — and rewriting it replaces a release's
        /// notes with a later release's commits. This says the section is one
        /// being prepared and the rewrite is meant.
        ///
        /// It does not cover a version that has been tagged. That release went
        /// out; its section is finished.
        #[arg(long)]
        force: bool,
    },
    /// Remove dependencies with the project's own package manager.
    ///
    /// Takes them out of every dependency field that lists them, out of the
    /// lockfile, and out of `node_modules`. A name the manifest never listed is
    /// not an error: the project ends up the way it was asked to be either way.
    ///
    /// `uninstall` is the same command. npm and pnpm both accept that spelling,
    /// and a reader who types it should not meet clap's "unrecognized
    /// subcommand" over a word two of the five managers call it by.
    #[command(alias = "uninstall")]
    Remove {
        /// The packages, by name.
        #[arg(value_name = "NAME", required = true)]
        names: Vec<String>,
    },
    /// List the routes under `app/`, or write a new one.
    ///
    /// A route is a directory and one to four reserved files, and both halves
    /// of this command read `crates/uf_router/src/reserved.rs` — the same
    /// grammar `uf build` discovers routes with. A generator with a list of
    /// its own would be a second answer to "what is a route called", and the
    /// first two disagreeing is what ubugeeei-prod/uf#224, #291 and #386 were.
    Routes {
        #[command(subcommand)]
        command: RoutesCommand,
    },
    /// Run a task from `uf.config.js`, or list them. Also `ufr`.
    Run {
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
        /// Run at most N tasks at once. Defaults to four, or the number of
        /// cores when that is fewer. `1` runs them one at a time.
        #[arg(long, short = 'j', value_name = "N")]
        concurrency: Option<usize>,
        /// Run every task, whatever `.uf/cache/task` holds. What they produce
        /// is still recorded.
        #[arg(long)]
        force: bool,
        /// Say, for each task, why it ran or was answered from the cache.
        #[arg(long)]
        why: bool,
        /// The task to run, as named under `tasks` in `uf.config.js`.
        /// Omit it to see what this project defines.
        script: Option<String>,
        /// Everything after the task name, handed to it untouched. A task
        /// that takes an option `uf run` also has is reached past `--`.
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
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,
    },
    /// Run the project's tests.
    Test {
        /// List what would run instead of running it.
        #[arg(long)]
        list: bool,
        /// Run in this mode, which chooses `.env.<mode>` and is what
        /// `import.meta.env.MODE` reads.
        #[arg(long, value_name = "MODE")]
        mode: Option<String>,

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
        /// Measure which of the project's Flow lines the suite executed.
        ///
        /// Counted by V8 and mapped back through the transform's source map,
        /// so the numbers are about the file you wrote and not about the
        /// JavaScript uf printed. Node only.
        #[arg(long)]
        coverage: bool,
        /// Write this coverage report; repeat for more than one.
        ///
        /// Overrides `test.coverage.reporters` in `uf.config.js`.
        #[arg(long = "coverage-reporter", value_name = "FORMAT", value_enum)]
        coverage_reporters: Vec<CoverageReporterArg>,
        /// Write coverage reports here instead of `test.coverage.directory`.
        #[arg(long, value_name = "DIR")]
        coverage_dir: Option<String>,
        /// Also write the run's results in this machine-readable format.
        #[arg(long, value_name = "FORMAT", value_enum, requires = "reporter_outfile")]
        reporter: Option<ResultReporterArg>,
        /// Where `--reporter` writes.
        #[arg(long, value_name = "FILE")]
        reporter_outfile: Option<String>,
        /// Only run files whose path contains one of these patterns.
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Update dependencies, and report the ones their ranges hold back.
    ///
    /// With no flag it is the manager's own update — everything moves to the
    /// newest version its declared range already allows, and `package.json` is
    /// not touched — followed by a report of what is newer than the ranges
    /// permit. That report is usually the answer people came for.
    ///
    /// `--latest`, `--minor` and `--patch` rewrite the ranges to the newest
    /// published version at or below that level, then install. Name packages to
    /// hold the rest still, or name none for all of them.
    Update {
        /// The packages to update; all of them when none is named.
        #[arg(value_name = "PACKAGE")]
        packages: Vec<String>,
        /// Rewrite ranges to the newest published version, then install.
        #[arg(long, alias = "major", group = "step")]
        latest: bool,
        /// Rewrite ranges as far as the next minor, then install.
        #[arg(long, group = "step")]
        minor: bool,
        /// Rewrite ranges as far as the next patch, then install.
        #[arg(long, group = "step")]
        patch: bool,
        /// Report what would change and change nothing — no update, no install.
        #[arg(long)]
        dry_run: bool,
    },
    /// The package manager underneath: what it is allowed to do, and what it
    /// has been told.
    ///
    /// `uf install`, `uf add` and the rest are the commands people run. This is
    /// the plumbing beside them — the settings a manager reads that uf has an
    /// opinion about.
    Pm {
        #[command(subcommand)]
        command: PmCommand,
    },
    /// Open a dependency for editing, and write the patch when you are done.
    ///
    /// `uf patch left-pad` prints a directory holding a copy of the package;
    /// edit it, then `uf patch --commit <that directory>` writes the patch and
    /// installs. The patch is reapplied by every install after that.
    ///
    /// pnpm and Yarn 2+ only. npm, bun and Yarn 1 have nothing equivalent, and
    /// uf names `patch-package` rather than installing it for you — this is the
    /// one command whose whole purpose is editing somebody else's code, and it
    /// is not the place for uf to add a dependency the project did not choose.
    Patch {
        /// The package to open, or with `--commit` the directory to commit.
        #[arg(value_name = "PACKAGE")]
        target: String,
        /// Write the patch from a directory `uf patch` opened, and install.
        #[arg(long)]
        commit: bool,
    },
    /// Show the versions a workspace shares, and change one everywhere.
    ///
    /// A package more than one manifest declares is a catalogue entry, and the
    /// range they agree on is its value. `uf catalog` prints them and every
    /// package whose manifests *dis*agree; `uf catalog set` makes them agree.
    ///
    /// pnpm's `catalog:` is a specifier only pnpm can resolve, so uf does not
    /// invent a fifth one: it reports how many a project has and leaves pnpm to
    /// resolve them. See ubugeeei-prod/uf#496.
    Catalog {
        #[command(subcommand)]
        command: Option<CatalogCommand>,
    },
    /// Replace this uf with the newest release.
    ///
    /// The command `uf upgrade` was named after and never was. It resolves the
    /// newest release, downloads it, checks it against the sha256 published
    /// beside it and links it — through the same installer
    /// `curl -fsSL https://setup.uniflowed.dev | sh` runs, because a second
    /// implementation of a checked download is a second one to get wrong. See
    /// ubugeeei-prod/uf#424 and ubugeeei-prod/uf#499.
    ///
    /// `UF_VERSION` pins a version, `UF_RELEASE_BASE` points at a mirror, and
    /// both mean here what they mean to the installer.
    SelfUpdate,
    /// Switch the active uf toolchain, for example `uf use uf@0.1.0`.
    ///
    /// A version this machine does not have is downloaded and verified, by the
    /// same installer `uf self-update` uses. It is not fabricated by copying
    /// the running binary under another name, which is what it used to be:
    /// ubugeeei-prod/uf#534.
    Use {
        /// The toolchain to activate.
        runtime: String,
    },
    /// List the installed dependency tree, through the project's own package
    /// manager.
    Ls {
        /// Package names, which narrow the tree to what depends on them.
        ///
        /// Names only. A manager's own flags are the manager's own surface and
        /// are reached with `uf exec` — uf cannot know which of them only read,
        /// and `pnpm audit --fix` rewrites a lockfile. Refusing a flag also
        /// keeps `uf ls --oops` at exit 2, uf failing to parse its own
        /// argument, rather than exit 1 from a manager uf handed a word to.
        #[arg(value_name = "NAME", trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Audit the installed tree for known vulnerabilities.
    ///
    /// Every manager uf supports has one, and each spells its severity
    /// threshold and its fix differently, so the flags are its own.
    Audit {
        /// Package names, where the manager narrows an audit to them.
        ///
        /// Names only, for the reason `uf ls` gives: every manager spells its
        /// severity threshold and its `--fix` differently, and one of those
        /// rewrites a lockfile.
        #[arg(value_name = "NAME", trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Search the registry.
    ///
    /// npm and pnpm can. Yarn and bun cannot, and uf says so rather than
    /// running a manager the project did not choose — a tool that quietly
    /// reaches for a different one is how a lockfile comes to be written by
    /// something nobody selected.
    Search {
        /// What to search for.
        #[arg(value_name = "TERM", required = true, trailing_var_arg = true)]
        terms: Vec<String>,
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

/// A coverage report `--coverage-reporter` can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum CoverageReporterArg {
    /// A table on the terminal, and nothing on disk.
    Text,
    /// `lcov.info`, which every code-host coverage integration reads.
    Lcov,
    /// `cobertura-coverage.xml`, which the JVM-shaped half of CI reads.
    Cobertura,
}

/// A machine-readable shape for the run's *results*, as opposed to its
/// coverage.
///
/// One value, and the reason there is only one: `uf test --json` is already
/// uf's own document and carries more than any of these could. What JUnit adds
/// is that no CI system has to be taught it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ResultReporterArg {
    /// JUnit XML, which every CI system parses for test results.
    Junit,
}

impl Commands {
    /// Whether this invocation must emit pure JSON on stdout.
    pub(crate) fn wants_json(&self) -> bool {
        matches!(
            self,
            Self::Check { json: true, .. }
                | Self::Doc { json: true, .. }
                | Self::Explain { json: true, .. }
                | Self::I18n {
                    command: I18nCommand::Extract { json: true, .. }
                        | I18nCommand::Merge { json: true, .. },
                }
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
            Self::Complete { .. }
            | Self::Completion { .. }
            | Self::Lsp
            | Self::Mcp
            | Self::Transform
            | Self::Assets => true,
            Self::Run { script, .. } => script.is_some(),
            _ => false,
        }
    }
}

/// What `uf routes` can do with the route table.
#[derive(Debug, Subcommand)]
pub(crate) enum RoutesCommand {
    /// Print the table `uf build` counts from.
    List,
    /// Write a route: the directory, its page, and whichever of the rest was
    /// asked for.
    ///
    /// The path is a URL path in the spelling the directories already use —
    /// `/articles/[slug]`, `/docs/[...path]`, `/(marketing)/about` — so what
    /// is typed is what appears in `RoutePath`. A spelling uf reserves without
    /// serving (`@team`, `(.)photo`) is refused here with the same sentence
    /// `uf build` and `uf lint` give, rather than written and reported later.
    ///
    /// Nothing is overwritten: a route whose page exists is an error, and a
    /// run that stops has written none of its files.
    Add {
        /// The route's URL path, as its directories spell it.
        #[arg(value_name = "PATH")]
        path: String,
        /// Also write `_uf.layout.js`: a wrapper for this path and everything
        /// under it.
        #[arg(long)]
        layout: bool,
        /// Give the page a `loader`, which runs before it renders.
        ///
        /// The one flag here that is not a file. A loader is an export of the
        /// page module rather than a name uf reserves, and the flag is still
        /// the right spelling: from where a reader stands the question is
        /// "does this route load data", and which of the answers happens to be
        /// a separate file is uf's business rather than theirs.
        #[arg(long)]
        loader: bool,
        /// Also write `_uf.middleware.js`: what runs before this path answers.
        #[arg(long)]
        middleware: bool,
    },
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
pub(crate) enum PmCommand {
    /// List the dependencies that would run code at install time, and approve
    /// the ones you have read.
    ///
    /// uf passes `--ignore-scripts` to every manager by default, so no
    /// dependency runs anything. Naming one here records it in the field your
    /// package manager reads — `pnpm.onlyBuiltDependencies`,
    /// `trustedDependencies`, `dependenciesMeta` — and the next install builds
    /// exactly those.
    ///
    /// npm and Yarn 1 have no per-package control: `--ignore-scripts` is all of
    /// them or none. uf says so rather than offering an approval that quietly
    /// means "and everything else too".
    #[command(name = "approve-builds")]
    ApproveBuilds {
        /// The packages to approve; none lists what is waiting.
        #[arg(value_name = "NAME")]
        names: Vec<String>,
        /// Say what would be approved, and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum CatalogCommand {
    /// Declare one range for a package in every manifest that has it.
    Set {
        /// The package.
        #[arg(value_name = "NAME")]
        name: String,
        /// The range, for example `^19.0.0`.
        #[arg(value_name = "RANGE")]
        range: String,
        /// Say what would change, and change nothing.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum I18nCommand {
    /// Write every message the project declares as one JSON catalogue.
    ///
    /// The file holds each message's MessageFormat 2 source, the parameters it
    /// takes, where it is declared and a digest of the two. A `translation`
    /// field starts equal to the source, which is what gives a translator
    /// something to edit rather than an empty box.
    ///
    /// A `message(…)` uf cannot read with certainty is reported and nothing is
    /// written: a catalogue quietly missing a message is invisible in review
    /// and visible to a reader of the page.
    Extract {
        /// The locale the messages are written in, e.g. `en-US`.
        ///
        /// Read from the project's `defineCatalogue` calls when they name one
        /// literal tag, and required when they name none or several.
        #[arg(long, value_name = "TAG")]
        locale: Option<String>,
        /// Where to write it. Defaults to `i18n/<locale>.json`.
        #[arg(long, value_name = "PATH")]
        out: Option<Utf8PathBuf>,
        /// Emit machine-readable JSON on stdout, and write no file.
        #[arg(long)]
        json: bool,
    },
    /// Read a translated catalogue back into a locale module.
    ///
    /// The file is the one `uf i18n extract` wrote, with each `translation`
    /// filled in and `locale` set to the language they are in. uf extracts the
    /// project again and holds every returned entry against what its message
    /// says today: one whose source or parameters changed while the file was
    /// out is named and left out, because a translation of a sentence that no
    /// longer exists is not a translation of the one that replaced it.
    Merge {
        /// The translated catalogue.
        #[arg(value_name = "FILE")]
        file: Utf8PathBuf,
        /// Where to write the module. Defaults to `<locale>.js` beside FILE.
        #[arg(long, value_name = "PATH")]
        out: Option<Utf8PathBuf>,
        /// Emit machine-readable JSON on stdout, and write no file.
        #[arg(long)]
        json: bool,
    },
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
                fix: false,
                fix_unsafe: false,
                paths: Vec::new()
            }
            .wants_json()
        );
        assert!(
            Commands::Check {
                json: true,
                fix: false,
                fix_unsafe: false,
                paths: Vec::new()
            }
            .wants_json()
        );
        assert!(
            !Commands::Build {
                size_report: false,
                mode: None,
                compile: false,
                target: None,
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
                mode: None,
                concurrency: None,
                force: false,
                why: false,
                script: Some("build".to_string()),
                args: Vec::new(),
            }
            .owns_stdout()
        );
        assert!(
            !Commands::Run {
                mode: None,
                concurrency: None,
                force: false,
                why: false,
                script: None,
                args: Vec::new(),
            }
            .owns_stdout(),
            "listing tasks renders, so it must keep stdout"
        );
        assert!(
            !Commands::Build {
                size_report: false,
                mode: None,
                compile: false,
                target: None,
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
