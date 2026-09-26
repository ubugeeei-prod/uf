//! `uf run` and `ufx`: the two commands that hand control to another process.

use std::borrow::Cow;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use uf_config::env_files::ProjectEnv;
use uf_config::{ResolvedConfig, TaskDefinition, load_config};
use uf_pm::{
    DetectionOptions, Operation, PackageManager, command_for, detect_package_manager_with,
};
use uf_task::arguments::Resolved;
use uf_task::{Concurrency, Plan, PlanError, PlanPackage, RunOptions, ScheduledTask, TaskCache};
use uf_term::prompt::{Choice, Outcome, Request, select};
use uf_term::{Cell, Column, Status, Table, Tone, display_width, truncate_to_width};

use crate::cli::CreateCommand;
use crate::commands::vite::load_project_config;
use crate::commands::{create, pm, test};
use crate::suggest::closest;
use crate::support::{DEVELOPMENT, plural, project_env, project_label};
use crate::ui::Ui;

mod pick;

/// What `uf run` was asked for beyond the task's name.
#[derive(Debug, Clone, Default)]
pub(crate) struct RunArgs {
    /// How many tasks may run at once.
    pub(crate) concurrency: Option<usize>,
    /// Ignore what the cache holds. It is still written.
    pub(crate) force: bool,
    /// Say, for every task, why it ran or did not.
    pub(crate) why: bool,
    /// Run the task in every workspace member that defines it.
    pub(crate) recursive: bool,
    /// Run it only in the members these select, which implies
    /// [`Self::recursive`].
    pub(crate) filter: Vec<String>,
    /// The words after the task's name fill none of its declared `args`:
    /// they are `uf prepare`'s staged files, appended as they are, and every
    /// declared argument takes its default. See [`pick::undeclared`].
    pub(crate) undeclared: bool,
}

/// One project a run can reach tasks in, loaded.
///
/// The project `uf run` started in, or a member of its workspace — each with
/// its own `uf.config.js`, its own `.env` files and its own `.uf/cache/task`,
/// because each is a project in its own right, and `uf run build` inside one
/// has always read exactly those.
struct Package {
    /// What `pkg#task` calls it. Empty for the project `uf run` started in
    /// when no `pkg#task` can name it: the workspace root, or a project with
    /// no workspace at all.
    name: CompactString,
    /// Where it is, relative to the workspace root.
    path: String,
    resolved: ResolvedConfig,
    /// Indices of the packages its `package.json` depends on.
    dependencies: Vec<usize>,
}

impl Package {
    fn new(
        resolved: ResolvedConfig,
        name: CompactString,
        path: String,
        dependencies: Vec<usize>,
    ) -> Self {
        Self {
            name,
            path,
            resolved,
            dependencies,
        }
    }

    /// The project `uf run` started in, on its own.
    fn alone(resolved: ResolvedConfig) -> Self {
        Self::new(
            resolved,
            CompactString::default(),
            String::new(),
            Vec::new(),
        )
    }
}

pub(crate) fn run_task(
    cwd: &Utf8Path,
    ui: &mut Ui,
    requested_mode: Option<&str>,
    script: &str,
    args: &[String],
    options: RunArgs,
) -> Result<()> {
    let resolved = load_project_config(cwd, requested_mode, DEVELOPMENT)?;

    // One project, unless the run asks for more or the plan reaches past it.
    // Finding a workspace walks the tree and loads every member's config, and
    // `uf run build` in a project whose `dependsOn` names no other package has
    // no reason to pay for either — or to be refused over a member's broken
    // config it was never going to read.
    let (packages, plan) = if options.recursive || !options.filter.is_empty() {
        let (packages, first_member) = workspace(resolved, requested_mode)?;
        let requests = requests_across(&packages, first_member, script, &options.filter)?;
        let plan = plan(&packages, &requests)?;
        (packages, plan)
    } else {
        match Plan::build(&resolved.config, script) {
            Ok(plan) => (vec![Package::alone(resolved)], plan),
            Err(PlanError::UnknownPackage { .. }) => {
                let started_in = resolved.root.clone();
                let (packages, _) = workspace(resolved, requested_mode)?;
                let current = packages
                    .iter()
                    .position(|package| package.resolved.root == started_in)
                    .unwrap_or(0);
                let plan = plan(&packages, &[(current, script)])?;
                (packages, plan)
            }
            Err(error) => {
                let packages = [Package::alone(resolved)];
                return Err(plan_error(&packages, error));
            }
        }
    };
    execute(ui, requested_mode, &packages, &plan, script, args, &options)
}

/// The workspace around `resolved`, loaded: every package a run can reach, and
/// the index of the first member among them.
///
/// The workspace root comes first, under no name, when it is the project `uf
/// run` started in — `uf run build` there means the root's own `build`, and
/// `-r` means the members'. A project that is itself a member is loaded once,
/// as that member, so a sibling's `pkg#task` and its own tasks are one node.
fn workspace(resolved: ResolvedConfig, mode: Option<&str>) -> Result<(Vec<Package>, usize)> {
    let Some((root, members)) = uf_project::enclosing_workspace(&resolved.root, &resolved.config)
    else {
        return Ok((vec![Package::alone(resolved)], 1));
    };
    let dependencies = uf_project::workspace_dependencies(&root, &members);

    let mut packages = Vec::with_capacity(members.len() + 1);
    let mut started_in = Some(resolved);
    if let Some(resolved) = started_in.take_if(|resolved| resolved.root == root) {
        packages.push(Package::alone(resolved));
    }
    let first_member = packages.len();
    for (member, depends_on) in members.iter().zip(dependencies) {
        let member_root = root.join(&member.path);
        let resolved = match started_in.take_if(|resolved| resolved.root == member_root) {
            Some(resolved) => resolved,
            None => load_project_config(&member_root, mode, DEVELOPMENT)?,
        };
        let depends_on = depends_on.into_iter().map(|at| at + first_member).collect();
        packages.push(Package::new(
            resolved,
            member.name.clone(),
            member.path.to_string(),
            depends_on,
        ));
    }
    Ok((packages, first_member))
}

/// Where `uf run <task> -r` runs the task: in every member, or in the members
/// `--filter` selects, that define it.
///
/// The workspace root is not one of them, which is pnpm's answer and the one
/// that keeps `-r` meaning one thing: the root's own `build` is `uf run build`.
fn requests_across<'s>(
    packages: &[Package],
    first_member: usize,
    script: &'s str,
    filter: &[String],
) -> Result<Vec<(usize, &'s str)>> {
    let members = &packages[first_member.min(packages.len())..];
    if members.is_empty() {
        bail!(
            "`-r` and `--filter` run a task across a workspace, and this project has no members\n\n  \
             a member is a directory with its own uf.config.js, or a package \
             package.json#workspaces lists"
        );
    }

    let selected: Vec<usize> = if filter.is_empty() {
        (first_member..packages.len()).collect()
    } else {
        let workspaces: Vec<uf_project::Workspace> = members
            .iter()
            .map(|member| uf_project::Workspace {
                name: member.name.clone(),
                path: member.path.clone().into(),
            })
            .collect();
        let dependencies: Vec<Vec<usize>> = members
            .iter()
            .map(|member| {
                member
                    .dependencies
                    .iter()
                    .map(|at| at - first_member)
                    .collect()
            })
            .collect();
        uf_project::select_workspaces(&workspaces, &dependencies, filter)
            .map_err(|error| anyhow!("{error}\n\n  members: {}", member_names(members)))?
            .into_iter()
            .map(|at| at + first_member)
            .collect()
    };

    let requests: Vec<(usize, &str)> = selected
        .into_iter()
        .filter(|&at| packages[at].resolved.config.tasks.contains_key(script))
        .map(|at| (at, script))
        .collect();
    if requests.is_empty() {
        let which = if filter.is_empty() {
            "workspace member"
        } else {
            "member --filter selects"
        };
        bail!(
            "no {which} defines a task {script:?}\n\n  members: {}",
            member_names(members)
        );
    }
    Ok(requests)
}

fn member_names(members: &[Package]) -> String {
    members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The plan for `requests` over `packages`, or the error a reader can act on.
fn plan(packages: &[Package], requests: &[(usize, &str)]) -> Result<Plan> {
    let reachable: Vec<PlanPackage<'_>> = packages
        .iter()
        .map(|package| PlanPackage {
            name: package.name.as_str(),
            path: package.path.as_str(),
            tasks: &package.resolved.config.tasks,
            dependencies: &package.dependencies,
        })
        .collect();
    Plan::build_workspace(&reachable, requests).map_err(|error| plan_error(packages, error))
}

fn plan_error(packages: &[Package], error: PlanError) -> anyhow::Error {
    match error {
        PlanError::Unknown { name, through } => {
            let asker = through.last().map(CompactString::as_str);
            let (owner, task) = owner_of(packages, &name, asker);
            anyhow!(unknown_task(
                &packages[owner].resolved,
                &name,
                task,
                &through
            ))
        }
        PlanError::UnknownPackage { reference, through } => {
            anyhow!(unknown_package(packages, &reference, &through))
        }
        PlanError::Cycle(cycle) => anyhow!(dependency_cycle(&cycle)),
    }
}

/// The package a task's label belongs to, and the task's name inside it.
///
/// `ui#build` is `ui`'s `build`. A bare name belongs to whoever asked for it —
/// the last label in the chain — or, when nobody did, to the first package,
/// which is the project `uf run` started in whenever that has no name.
fn owner_of<'l>(packages: &[Package], label: &'l str, asker: Option<&str>) -> (usize, &'l str) {
    let named = |label: &str| {
        let (owner, _) = label.split_once('#')?;
        packages
            .iter()
            .position(|package| !package.name.is_empty() && package.name == owner)
    };
    if let (Some(owner), Some((_, task))) = (named(label), label.split_once('#')) {
        return (owner, task);
    }
    (asker.and_then(named).unwrap_or(0), label)
}

/// The error for a `pkg#task` whose package the workspace does not have.
fn unknown_package(packages: &[Package], reference: &str, through: &[CompactString]) -> String {
    let owner = reference
        .split_once('#')
        .map_or(reference, |(owner, _)| owner);
    let asker = through.last().map_or("a task", CompactString::as_str);
    let names: Vec<&str> = packages
        .iter()
        .map(|package| package.name.as_str())
        .filter(|name| !name.is_empty())
        .collect();

    let mut message = format!(
        "{asker:?} depends on {reference:?}, and there is no workspace member named {owner:?}"
    );
    if names.is_empty() {
        message.push_str(
            "\n\n  this project is not part of a workspace: a member is a directory with its own \
             uf.config.js, or a package package.json#workspaces lists",
        );
        return message;
    }
    let suggestions = closest(owner, names.iter().copied());
    if !suggestions.is_empty() {
        message.push_str("\n\n  did you mean: ");
        message.push_str(&suggestions.join(", "));
    }
    message.push_str("\n\n  members: ");
    message.push_str(&names.join(", "));
    message
}

/// Run a plan that has been built, and report it.
fn execute(
    ui: &mut Ui,
    mode: Option<&str>,
    packages: &[Package],
    plan: &Plan,
    script: &str,
    args: &[String],
    options: &RunArgs,
) -> Result<()> {
    // Every requested task's arguments, asked for if need be, before anything
    // else is loaded or started.
    let given = task_arguments(ui, packages, plan, args, options)?;

    // Each package's environment, loaded when its first task is scheduled. A
    // task is project code with a shell in front of it, so it reads its
    // project's `.env` files like everything else uf runs, with the runtime
    // that project's `uf.config.js` declares in front of `PATH`. `development`
    // is the default because a task is something a person runs at a terminal;
    // a task that runs `uf build` gets `production` from that command, because
    // the values uf injected here are marked as uf's and lose to a file. See
    // `uf_config::env_files`.
    //
    // After planning, and only for the packages the plan runs a task in:
    // resolving a declared runtime can mean installing one, and neither a
    // mistyped task name nor a member no task reaches is a reason to — nor is
    // that member's `.env.development` failing to parse.
    let mut envs: BTreeMap<usize, ProjectEnv> = BTreeMap::new();
    let mut tasks = Vec::with_capacity(plan.len());
    for (at, node) in plan.nodes().iter().enumerate() {
        let package = &packages[node.package];
        let name = node.name.as_str();
        let Some(definition) = package.resolved.config.tasks.get(name) else {
            bail!(unknown_task(&package.resolved, &node.label, name, &[]));
        };
        let details = definition.details();
        if details.is_some_and(|task| task.cache == Some(true) && task.inputs.is_empty()) {
            bail!(
                "task {:?} sets `cache: true` and declares no `inputs`\n\n  \
                 uf keys a cached result on the files a task says it reads, so a task \
                 that names none\n  cannot be cached — and a request to cache it that \
                 uf quietly ignored would be worse\n  than this message. Add `inputs`, \
                 or drop the `cache` field.",
                node.label
            );
        }
        // Only a requested task takes the caller's arguments, and it takes
        // them in the command text so that two runs with different arguments
        // are two cache keys rather than one.
        let mut command = definition.command().to_string();
        if let Some(given) = given.get(&at).filter(|given| !given.is_empty()) {
            command.push(' ');
            command.push_str(&given.command_text());
        }
        let env = match envs.entry(node.package) {
            Entry::Occupied(slot) => slot.into_mut(),
            Entry::Vacant(slot) => {
                let env = project_env(&package.resolved, mode, DEVELOPMENT)?;
                slot.insert(runtime_environment(&package.resolved, ui, env)?)
            }
        };
        let mut given = environment_of(env);
        if let Some(details) = details {
            for (key, value) in &details.env {
                given.task(key, value);
            }
            if let Some(cwd) = &details.cwd {
                given.directory(cwd);
            }
        }
        tasks.push(ScheduledTask {
            name: node.name.clone(),
            label: node.label.clone(),
            package: node.package,
            root: package.resolved.root.clone(),
            dependencies: node.dependencies.clone(),
            keyed_on: node.across.clone(),
            command,
            inputs: details.map(|task| task.inputs.clone()).unwrap_or_default(),
            outputs: details.map(|task| task.outputs.clone()).unwrap_or_default(),
            cacheable: definition.is_cacheable(),
            environment: given,
        });
    }

    let spawner = TaskSpawner {
        packages,
        envs: &envs,
        requested: plan
            .requested()
            .iter()
            .map(|&at| {
                let words = given.get(&at).map(Resolved::words).unwrap_or_default();
                (
                    plan.nodes()[at].package,
                    plan.nodes()[at].name.clone(),
                    words,
                )
            })
            .collect(),
    };
    let reporter = Reporter {
        // One task is the whole plan, so there is nothing to schedule and
        // nothing to report that the task itself does not already say. This is
        // what keeps `uf run build` the command it has always been — unless
        // the caller asked why, which is a question uf must answer even when
        // there is only one task to answer it about.
        quiet: plan.len() == 1 && !options.why,
        why: options.why,
        width: plan
            .nodes()
            .iter()
            .map(|node| node.label.chars().count())
            .max()
            .unwrap_or(0),
    };
    if options.why {
        print_workspace_plan(plan);
    }
    let started = std::time::Instant::now();
    // Before the run adds to them, once for each project the run writes into.
    // See `uf_infra::cache` for the bound and #218 for why every cache under
    // `.uf/cache` now has one.
    let mut swept = BTreeSet::new();
    for task in &tasks {
        if swept.insert(&task.root) {
            TaskCache::open(&task.root).sweep();
        }
    }
    let report = uf_task::run(
        &tasks,
        RunOptions {
            concurrency: match options.concurrency {
                Some(count) => Concurrency::Fixed(count),
                None => Concurrency::Auto,
            },
            force: options.force,
        },
        &spawner,
        &reporter,
    );

    if plan.len() > 1 {
        let replayed = report.replayed();
        let unreached = report
            .outcomes
            .iter()
            .filter(|outcome| matches!(outcome.decision, uf_task::Decision::NotRun))
            .count();
        let mut line = format!(
            "  {}, {replayed} replayed, {} run",
            plural(plan.len(), "task"),
            plan.len() - replayed - unreached,
        );
        if unreached > 0 {
            line.push_str(&format!(", {unreached} not reached"));
        }
        let _ = writeln!(
            std::io::stderr(),
            "{line}, in {}",
            seconds(started.elapsed().as_micros() as u64),
        );
    }

    let failures = report.failures();
    if failures.is_empty() {
        return Ok(());
    }
    // The first failure in plan order is the one to lead with: it is the one
    // whose output the reader has just watched go past.
    let mut message = match &failures[0].status {
        uf_task::Status::Failed(said) => said.clone(),
        _ => format!("task {script:?} failed"),
    };
    if failures.len() > 1 {
        message.push('\n');
        for failure in &failures[1..] {
            if let uf_task::Status::Failed(said) = &failure.status {
                message.push_str("\n  ");
                message.push_str(said);
            }
        }
    }
    bail!(message)
}

/// The words each requested task runs with, by plan index: the caller's
/// arguments sorted into what the task declares, with anything missing asked
/// for at a terminal.
///
/// All of it before anything starts. A question in the middle of a run would
/// sit under the output of whatever was already running, and an answer that
/// turned out to be refused would leave half a plan run.
fn task_arguments(
    ui: &mut Ui,
    packages: &[Package],
    plan: &Plan,
    args: &[String],
    options: &RunArgs,
) -> Result<BTreeMap<usize, Resolved>> {
    let mut given = BTreeMap::new();
    let mut answers = BTreeMap::new();
    let mut asked = false;
    for &at in plan.requested() {
        let node = &plan.nodes()[at];
        let package = &packages[node.package];
        let declared = package
            .resolved
            .config
            .tasks
            .get(node.name.as_str())
            .map_or(&[][..], TaskDefinition::args);
        let resolved = if options.undeclared {
            pick::undeclared(&node.label, declared, args)?
        } else {
            let (resolved, this) = pick::resolve(
                &node.label,
                declared,
                args,
                &mut pick::Terminal,
                &mut answers,
            )?;
            asked |= this;
            resolved
        };
        given.insert(at, resolved);
    }

    // A person who picked the values is shown what they add up to, and how to
    // say it next time without being asked. Only then: a run whose arguments
    // were all typed prints exactly what it always did.
    if asked {
        for (&at, resolved) in &given {
            let node = &plan.nodes()[at];
            let Some(definition) = packages[node.package]
                .resolved
                .config
                .tasks
                .get(node.name.as_str())
            else {
                continue;
            };
            let mut command = definition.command().to_string();
            if !resolved.is_empty() {
                command.push(' ');
                command.push_str(&resolved.command_text());
            }
            let replay = pick::replay(&node.label, resolved);
            ui.render_err(|renderer, out| {
                renderer.status(out, Status::Info, &format!("{}: {command}", node.label));
                let muted = renderer.theme().muted;
                renderer.line(out, muted, &format!("  next time: {replay}"));
            });
        }
    }
    Ok(given)
}

/// With `--why`, a plan that spans packages is printed before it runs.
///
/// What each task waits for is the half of "why" the lines after it cannot
/// show: they say why a task ran, and not why it ran *then* — which, across a
/// workspace, is the order of the packages' dependencies, and is the thing to
/// check when a package was built against a stale sibling.
fn print_workspace_plan(plan: &Plan) {
    let packages = plan
        .nodes()
        .iter()
        .map(|node| node.package)
        .collect::<BTreeSet<_>>()
        .len();
    if packages < 2 {
        return;
    }
    let width = plan
        .nodes()
        .iter()
        .map(|node| node.label.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = format!(
        "  {} across {}\n",
        plural(plan.len(), "task"),
        plural(packages, "package")
    );
    for node in plan.nodes() {
        if node.dependencies.is_empty() {
            out.push_str(&format!("  {}\n", node.label));
            continue;
        }
        let after = node
            .dependencies
            .iter()
            .map(|&at| plan.nodes()[at].label.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("  {:width$}  after {after}\n", node.label));
    }
    let _ = write!(std::io::stderr(), "{out}");
}

/// The workspace a run here can span, as one line for `uf explain run`: each
/// member, how many tasks it defines, and which members it runs after.
///
/// [`None`] when there is no workspace, so a project without one is explained
/// exactly as it was.
pub(crate) fn workspace_summary(resolved: &ResolvedConfig) -> Option<String> {
    let (root, members) = uf_project::enclosing_workspace(&resolved.root, &resolved.config)?;
    let dependencies = uf_project::workspace_dependencies(&root, &members);
    let parts = members
        .iter()
        .zip(&dependencies)
        .map(|(member, depends_on)| {
            let defined =
                load_config(root.join(&member.path)).map_or(0, |member| member.config.tasks.len());
            let mut part = format!("{} ({})", member.name, plural(defined, "task"));
            if !depends_on.is_empty() {
                let after = depends_on
                    .iter()
                    .map(|&at| members[at].name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                part.push_str(" after ");
                part.push_str(&after);
            }
            part
        })
        .collect::<Vec<_>>();
    Some(format!(
        "{} — {}; `uf run <task> -r` runs a task in each member that defines it, in that \
         order, `--filter` selects members, and `pkg#task` in `dependsOn` names one",
        plural(members.len(), "member"),
        parts.join("; ")
    ))
}

/// The environment every task of one package starts with: its mode and its
/// `.env` values.
///
/// Per package, because each member reads its own `.env` files. Every value is
/// digested as it goes in, so nothing a task's note keeps can be read back as
/// one: see `uf_task::Environment`, and #1006 for what keeping them looked like.
fn environment_of(env: &ProjectEnv) -> uf_task::Environment {
    let mut environment = uf_task::Environment::new(env.mode());
    for (name, value) in env.values() {
        environment.file(name, value);
    }
    environment
}

/// The error for `dependsOn` that closes a loop.
fn dependency_cycle(cycle: &[compact_str::CompactString]) -> String {
    let path = cycle
        .iter()
        .map(compact_str::CompactString::as_str)
        .collect::<Vec<_>>()
        .join(" → ");
    format!(
        "`dependsOn` in uf.config.js closes a loop: {path}\n\n  \
         each of these waits for the next, so none of them can start"
    )
}

/// Turns a task into the process that runs it.
///
/// The two engines live here rather than in `uf_task` because which of them
/// runs a task is `uf run`'s question: a task that names a command is uf's,
/// and a task that names none is Vite Task's.
///
/// So is the third question, which is what "a task that names a command is
/// uf's" means. It used to mean `sh -c`, which is not a thing every machine
/// has: `uf run` did not work on Windows at all, for any task, however simple.
/// [`uf_task::parse`] reads the command instead, and a command that is a
/// program and its arguments — every one in this repository — uf starts
/// itself. See [`TaskSpawner::started`] and [`TaskSpawner::shell`].
///
/// And, since a run can span a workspace, a fourth: *whose* task it is. Each
/// task runs in its own package's root, with that package's `.env` values —
/// what `uf run build` inside the package would have given it.
struct TaskSpawner<'a> {
    packages: &'a [Package],
    /// The environment of each package the plan runs a task in.
    envs: &'a BTreeMap<usize, ProjectEnv>,
    /// The tasks that were asked for, by package and name, with the words
    /// each is run with: the only ones that take the caller's arguments.
    requested: Vec<(usize, CompactString, Vec<String>)>,
}

impl uf_task::Spawn for TaskSpawner<'_> {
    fn command(&self, scheduled: &ScheduledTask) -> std::io::Result<ProcessCommand> {
        let package = self.packages.get(scheduled.package).ok_or_else(|| {
            std::io::Error::other(format!("task {:?} belongs to no package", scheduled.label))
        })?;
        let env = self.envs.get(&scheduled.package).ok_or_else(|| {
            std::io::Error::other(format!(
                "task {:?} was given no environment",
                scheduled.label
            ))
        })?;
        let task = package
            .resolved
            .config
            .tasks
            .get(scheduled.name.as_str())
            .ok_or_else(|| {
                std::io::Error::other(format!("task {:?} is not defined", scheduled.label))
            })?;

        // A task that names a command is run by uf, because `uf.config.js` is
        // where its meaning is written down and Vite Task has no way to read
        // it — handing `ci` to `vp run ci` asked Vite+ for a script it had
        // never heard of, so every task defined here failed on a machine that
        // had `vp` and on one that did not. A task with no command of its own
        // is Vite+'s, and is handed over — which runs the `package.json`
        // script of that name, so it is a hand-over a project can refuse.
        if task.command().trim().is_empty() {
            if !package.resolved.config.task_runner.allow_package_scripts {
                return Err(std::io::Error::other(
                    "it has no `command`, and `taskRunner.allowPackageScripts` is false\n\n  \
                     A task with no command of its own is handed to Vite+'s task runner, \
                     `vp run`,\n  which runs the `package.json` script of that name. Give \
                     the task a `command`,\n  or allow package scripts.",
                ));
            }
            return Ok(self.vite_task(package, env, scheduled));
        }

        let details = task.details();
        // The directory the task runs in, which is also what a program written
        // as a relative path is relative to.
        let directory = match details.and_then(|details| details.cwd.as_ref()) {
            Some(cwd) => package.resolved.root.join(cwd.as_str()),
            None => package.resolved.root.clone(),
        };

        let command = scheduled.command.as_str();
        let parsed = uf_task::parse(command);
        let (mut process, inline) = match &parsed {
            uf_task::Command::Direct(direct) => {
                (Self::started(direct, &directory), &direct.assignments[..])
            }
            uf_task::Command::Shell(syntax) => {
                let found = posix_shell(ShellLookup::HOST, std::env::var_os("PATH").as_deref());
                (Self::shell(command, *syntax, found.as_deref())?, &[][..])
            }
            // These read as the second half of the runner's own "task {name}
            // could not be started:", which is why neither names the task
            // again.
            uf_task::Command::Nothing => {
                return Err(std::io::Error::other(
                    "its `command` is a comment, or is empty\n\n  \
                     A task that runs nothing is a task whose check nobody would notice \
                     going missing.",
                ));
            }
            uf_task::Command::Malformed(why) => {
                return Err(std::io::Error::other(format!(
                    "its command cannot be read — {why}\n\n    {command}"
                )));
            }
        };

        let overrides: Vec<(&str, &str)> = details
            .map(|details| details.env.iter())
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            // `A=1 prog` after `env: { A: 2 }` is the shell's own precedence:
            // what is written on the command line is the more specific of the
            // two, and `apply_over` lets the later entry win.
            .chain(
                inline
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .collect();

        // Under the task's own `env`, which is the more specific of the two: a
        // task that names a variable means it, and a `.env` file is the
        // project's default rather than an override.
        //
        // In one call rather than two, because setting the files and then the
        // overrides gets the values right and the label wrong: `apply` also
        // writes `UF_ENV_INJECTED`, which tells a nested uf "these came from a
        // file, your own files may overrule them". A task's `env` did not come
        // from a file, so a marker naming it let `.env.production` win over the
        // task inside a nested `uf build` — the exact override the task was
        // written to make.
        env.apply_over(&mut process, &overrides);
        process.current_dir(&directory);
        Ok(process)
    }
}

/// Whether a direct command's program is a path rather than a name to look up.
///
/// Both separators on both platforms, deliberately. `/` is a separator on
/// Windows as well as everywhere else, and a program written with a `\` is a
/// Windows path in a config no Unix machine can run either way — so resolving
/// it against the task's directory fails by naming a file that is not there,
/// which is a better answer than a `PATH` lookup for a name with a backslash
/// in it.
fn is_path(program: &str) -> bool {
    program.contains('/') || program.contains('\\')
}

/// Where a POSIX shell comes from on this platform.
///
/// The same arrangement as [`BinPlatform`], for the same reason: a value
/// rather than a `#[cfg]`, so both answers are exercised by the tests on a
/// machine that has only one of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShellLookup {
    /// `sh` is part of the platform. `Command` finds it, as `uf run` always
    /// has, and uf has nothing useful to add by looking first.
    OnThePath,
    /// `sh` is a program that may or may not be installed, so uf looks — not
    /// to find it faster, but so that its *absence* can be reported as the
    /// thing it is rather than as "os error 2" against a program the reader
    /// never wrote.
    Searched,
}

impl ShellLookup {
    /// The rules uf is actually running under.
    const HOST: Self = if cfg!(windows) {
        Self::Searched
    } else {
        Self::OnThePath
    };
}

/// What a searched-for POSIX shell may be called, in the order to try.
const SHELL_NAMES: [&str; 2] = ["sh.exe", "sh"];

/// A POSIX shell from `path`, or [`None`] when this machine has none.
///
/// `path` is the raw `PATH`, passed in rather than read here so that a test
/// can hand it a directory it built.
fn posix_shell(lookup: ShellLookup, path: Option<&std::ffi::OsStr>) -> Option<Utf8PathBuf> {
    match lookup {
        // Bare, so the spawn is byte for byte the one `uf run` has always
        // done on a machine that has a shell.
        ShellLookup::OnThePath => Some(Utf8PathBuf::from("sh")),
        ShellLookup::Searched => std::env::split_paths(path?)
            .filter_map(|directory| Utf8PathBuf::from_path_buf(directory).ok())
            .flat_map(|directory| SHELL_NAMES.map(|name| directory.join(name)))
            .find(|candidate| candidate.is_file()),
    }
}

impl TaskSpawner<'_> {
    /// A command uf starts itself: no shell, on any platform.
    ///
    /// The program is resolved against the task's directory when it is written
    /// as a path, rather than left to [`ProcessCommand`]. std documents that
    /// pairing a relative program with `current_dir` is "platform specific and
    /// unstable" — Unix resolves it against the child's directory and Windows
    /// against the parent's — and `tools/upstream/sync.sh`, which is how most
    /// of this repository's tasks are written, has to mean one file.
    fn started(direct: &uf_task::Direct, directory: &Utf8Path) -> ProcessCommand {
        let program = if is_path(&direct.program) {
            Cow::Owned(directory.join(&direct.program))
        } else {
            Cow::Borrowed(Utf8Path::new(direct.program.as_str()))
        };
        let mut process = ProcessCommand::new(program.as_std_path());
        process.args(&direct.args);
        process
    }

    /// A command uf does not run itself, handed to a POSIX shell.
    ///
    /// One shell language on every platform, and that is the decision this
    /// makes. Every task command ever written for `uf run` was written for
    /// `sh`, because `sh -c` is what `uf run` was; `cmd.exe` reads `;`, `$VAR`
    /// and quotes differently, so handing it the same string would not be a
    /// Windows port of this behaviour but a second, silent meaning for it.
    /// uf looks for a POSIX shell and, when there is none, says which
    /// construct made one necessary.
    ///
    /// # Errors
    ///
    /// When this machine has no POSIX shell — `found` is [`None`]. The message
    /// is the second half of the runner's "task {name} could not be started:",
    /// so it does not name the task again.
    fn shell(
        command: &str,
        syntax: uf_task::ShellSyntax,
        found: Option<&Utf8Path>,
    ) -> std::io::Result<ProcessCommand> {
        let Some(shell) = found else {
            return Err(std::io::Error::other(format!(
                "it needs a shell, and this machine has none\n\n  \
                 its command uses {syntax}, which uf does not run itself:\n\n    \
                 {command}\n\n  \
                 uf starts a task's command directly when it is a program and its \
                 arguments,\n  which needs no shell anywhere. Anything else is `sh -c`, \
                 and there is no `sh`\n  on PATH here — Git for Windows ships one. The \
                 other way out is to write the\n  task as something uf can start: a \
                 script, or two tasks joined by `dependsOn`."
            )));
        };
        let mut process = ProcessCommand::new(shell.as_std_path());
        process.arg("-c").arg(command);
        Ok(process)
    }

    fn vite_task(
        &self,
        package: &Package,
        env: &ProjectEnv,
        scheduled: &ScheduledTask,
    ) -> ProcessCommand {
        let runner = std::env::var_os("UF_VITE_TASK_BIN").unwrap_or_else(|| "vp".into());
        let mut process = ProcessCommand::new(runner);
        env.apply(&mut process);
        process.arg("run").arg(scheduled.name.as_str());
        // Only a task that was asked for takes the caller's arguments; a
        // dependency was not the thing they typed them after.
        let words = self
            .requested
            .iter()
            .find(|(at, name, _)| *at == scheduled.package && *name == scheduled.name)
            .map(|(_, _, words)| words.as_slice())
            .unwrap_or_default();
        if !words.is_empty() {
            process.arg("--").args(words);
        }
        process.current_dir(package.resolved.root.as_std_path());
        process
    }
}

/// What uf itself says while a plan runs.
///
/// On stderr, all of it. `uf run <task>` hands stdout to the task — that is
/// what `Commands::owns_stdout` records — and a scheduler's notes on a
/// pipeline's output would be uf writing on a report it did not produce.
struct Reporter {
    quiet: bool,
    why: bool,
    width: usize,
}

impl uf_task::Observe for Reporter {
    fn finished(&self, outcome: &uf_task::TaskOutcome) {
        if self.quiet {
            return;
        }
        let mark = match &outcome.status {
            uf_task::Status::Succeeded => "✓",
            uf_task::Status::Failed(_) => "✗",
            uf_task::Status::NotRun => "·",
        };
        let took = match &outcome.decision {
            uf_task::Decision::Replayed { saved_micros } => {
                format!("saved {}", seconds(*saved_micros))
            }
            _ => seconds(outcome.duration_micros),
        };
        let mut line = format!(
            "{mark} {:width$}  {:8}  {took}",
            outcome.name,
            outcome.decision.verb(),
            width = self.width,
        );
        if self.why {
            match &outcome.decision {
                uf_task::Decision::Replayed { .. } => {
                    line.push_str("  — every declared input is unchanged");
                }
                uf_task::Decision::Ran(reason) => {
                    line.push_str(&format!("  — {reason}"));
                }
                uf_task::Decision::NotRun => {
                    line.push_str("  — an earlier task failed");
                }
            }
        }
        let _ = writeln!(std::io::stderr(), "{line}");
    }
}

/// A microsecond count as a short duration.
fn seconds(micros: u64) -> String {
    let seconds = micros as f64 / 1_000_000.0;
    if seconds < 10.0 {
        format!("{seconds:.2}s")
    } else {
        format!("{seconds:.1}s")
    }
}

/// Widest a task's command is shown at on the menu.
///
/// Shorter than [`COMMAND_WIDTH`] because the menu also carries a name column
/// and a two-space gutter, and a row that wraps stops being a row.
const MENU_COMMAND_WIDTH: usize = 44;

/// Every task this project defines: its name, and what it runs.
///
/// Returns nothing rather than an error when there is no config or it does not
/// load. This is asked on the way to drawing a menu, and a directory that is
/// not a uf project should get a menu of uf's commands rather than a parse
/// error about a file the reader has not written yet.
pub(crate) fn task_names(cwd: &Utf8Path) -> Vec<(String, String)> {
    let Ok(resolved) = load_config(cwd) else {
        return Vec::new();
    };
    resolved
        .config
        .tasks
        .iter()
        .map(|(name, task)| (name.to_string(), elide(&runs(task), MENU_COMMAND_WIDTH)))
        .collect()
}

/// What a task runs, as a list shows it: the command, then the arguments it
/// declares — `node scripts/deploy.js <target> [region=eu]` — so a reader
/// choosing it can see what they will be asked for.
///
/// A task with no command of its own is Vite Task's, and saying so is more
/// useful than an empty cell.
fn runs(task: &TaskDefinition) -> String {
    let command = task.command().trim();
    let mut runs = if command.is_empty() {
        String::from("vite task")
    } else {
        command.to_owned()
    };
    if !task.args().is_empty() {
        runs.push(' ');
        runs.push_str(&pick::signature(task.args()));
    }
    runs
}

/// `uf run` with no task: pick one at a terminal, and list them anywhere else.
///
/// Picking flows straight into the task's own arguments — [`run_task`] asks
/// for whatever it declares and was not given — so choosing `deploy` here and
/// typing `uf run deploy` are the same run from that point on. Everything
/// else, the flags included, is exactly what `uf run <task>` would have had.
pub(crate) fn pick_task(
    cwd: &Utf8Path,
    ui: &mut Ui,
    mode: Option<&str>,
    options: RunArgs,
) -> Result<()> {
    let tasks = task_names(cwd);
    if tasks.is_empty() {
        return list_tasks(cwd, ui);
    }
    let choices: Vec<Choice<'_>> = tasks
        .iter()
        .map(|(name, runs)| Choice::new(name, runs))
        .collect();
    match select(&Request::new("Which task?", &choices)) {
        Outcome::Chose(choice) => run_task(cwd, ui, mode, choice.name, &[], options),
        // No terminal: what `uf run` printed before there was a picker.
        Outcome::NotInteractive => list_tasks(cwd, ui),
        Outcome::Cancelled => Ok(()),
    }
}

/// Widest a command is printed at in the task table.
///
/// A task may legitimately be a hundred characters of shell — this repository
/// has one — and printing it in full pushes every column after it off the
/// screen. The name is what the reader needs; the command is context.
const COMMAND_WIDTH: usize = 56;

/// `text`, cut to `width` columns with a trailing ellipsis if it did not fit.
fn elide(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_owned();
    }
    let mut out = truncate_to_width(text, width.saturating_sub(1)).to_owned();
    out.push('…');
    out
}

/// `uf run` with no task name: what this project can do.
///
/// A tool that requires a name before it will tell you the names is a tool you
/// have to read the config to use. This is the answer to "what can I run here",
/// and it is the same list the unknown-task error points at.
pub(crate) fn list_tasks(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_project_config(cwd, None, DEVELOPMENT)?;
    let tasks = &resolved.config.tasks;

    if tasks.is_empty() {
        ui.render(|renderer, out| {
            renderer.banner(out, "uf run", None);
            renderer.status(
                out,
                Status::Info,
                "this project defines no tasks; add them under `tasks` in uf.config.js",
            );
        });
        return Ok(());
    }

    // Owned first, borrowed second: `Table` holds `&str`, so every cell's text
    // has to outlive the table rather than be built into it.
    let rows = tasks
        .iter()
        .map(|(name, task)| {
            let after = task
                .depends_on()
                .iter()
                .map(compact_str::CompactString::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            (name.to_string(), elide(&runs(task), COMMAND_WIDTH), after)
        })
        .collect::<Vec<_>>();

    let mut table = Table::new(vec![
        Column::left("task"),
        Column::left("runs"),
        Column::left("after"),
    ]);
    for (name, runs, after) in &rows {
        table.push(vec![
            Cell::toned(name, Tone::Accent),
            Cell::new(runs),
            Cell::toned(after, Tone::Muted),
        ]);
    }

    let count = tasks.len();
    ui.render(|renderer, out| {
        renderer.banner(out, "uf run", Some(project_label(&resolved.root)));
        renderer.table(out, 2, &table);
        renderer.blank(out);
        renderer.status(
            out,
            Status::Info,
            &format!("{}; run one with `uf run <task>`", plural(count, "task")),
        );
    });

    Ok(())
}

/// The error for a task name that is not in `uf.config.js`.
///
/// `task "biuld" is not defined in uf.config.js` is true and unhelpful: the
/// reader knows what they typed, and what they want is the name they meant.
/// So the message names the closest tasks, and — when there are few enough to
/// read — every task the project defines, because someone who has just arrived
/// in a repository does not know what is on offer and should not have to open
/// the config to find out.
///
/// `label` is what the message calls the task — `ui#biuld` across a workspace —
/// and `script` is its name inside `resolved`, which is what the suggestions
/// are measured against.
fn unknown_task(
    resolved: &ResolvedConfig,
    label: &str,
    script: &str,
    through: &[compact_str::CompactString],
) -> String {
    let names = resolved
        .config
        .tasks
        .keys()
        .map(compact_str::CompactString::as_str)
        .collect::<Vec<_>>();

    let mut message = match through.last() {
        // A name nobody typed is a name somebody's `dependsOn` asked for, and
        // the reader's first question is which task that was.
        Some(asker) => {
            format!("task {label:?} is not defined in uf.config.js, and {asker:?} depends on it")
        }
        None => format!("task {label:?} is not defined in uf.config.js"),
    };
    if names.is_empty() {
        message.push_str("\n\n  this project defines no tasks");
        return message;
    }

    let suggestions = closest(script, names.iter().copied());
    if !suggestions.is_empty() {
        message.push_str("\n\n  did you mean: ");
        message.push_str(&suggestions.join(", "));
    }

    // A long list is a wall of text rather than an answer; past this many, the
    // suggestions are the help and `uf run` with no task name is where the rest
    // lives.
    const LISTED_IN_FULL: usize = 12;
    if names.len() <= LISTED_IN_FULL {
        message.push_str("\n\n  tasks: ");
        message.push_str(&names.join(", "));
    } else {
        message.push_str(&format!(
            "\n\n  {} tasks are defined; `uf run` lists them",
            names.len()
        ));
    }
    message
}

/// `uf exec PACKAGE [ARGS...]`, also spelled `ufx`.
///
/// Four paths, tried in order, and every one of them either runs something or
/// fails:
///
///  1. a package uf implements itself, run in this process;
///  2. a binary the project has already installed, in `node_modules/.bin`;
///  3. a path the caller gave, executed as written;
///  4. a package that is not installed — fetched and run through the detected
///     package manager, but only when the caller said `--yes`.
///
/// # What this used to do
///
/// It wrote `.uf/exec-cache/<package>.json`, printed "cached execution request
/// for registry resolution", and exited 0 having run nothing. Nothing ever
/// read that file back, so the directory was named a cache and cached nothing;
/// it existed so that a command with nothing to do had something to write. A
/// CI job whose step was `ufx some-codegen` went green having generated
/// nothing. See ubugeeei-prod/uf#274.
///
/// # Why fetching asks first
///
/// Fetching a package that is not in the lockfile and running its binary is
/// the most dangerous thing a package manager does, and uf has already taken a
/// position one step later: `pm.allowLifecycleScripts` is false by default and
/// `uf install` passes `--ignore-scripts` through to whichever manager runs.
/// Downloading and executing an unpinned name silently would be the same hole
/// one step earlier, so it needs `--yes` — the flag `npx` spells the same way,
/// for the same reason. `docs/security.md` records the decision.
///
/// A binary in `node_modules/.bin` needs no such consent: it is already
/// installed, already in the tree the lockfile pins, and running it is what
/// `npm exec` and `pnpm exec` do without asking.
/// `env`, with `runtime`'s release in front of `PATH` when `uf.config.js`
/// declares one.
///
/// A task and a `ufx` binary are project code: a `node` they start, and every
/// `#!/usr/bin/env node` script they run, should be the Node the project says
/// it runs on. Only a declared runtime changes anything — a project with none
/// runs its tasks exactly as before, and a machine with no JavaScript host at
/// all can still run a task that needs none. See ubugeeei-prod/uf#940.
fn runtime_environment(
    resolved: &uf_config::ResolvedConfig,
    ui: &mut Ui,
    env: uf_config::env_files::ProjectEnv,
) -> Result<uf_config::env_files::ProjectEnv> {
    use crate::commands::runtimes::{self, Role};

    if Role::Runtime.declared(&resolved.config).is_none() {
        return Ok(env);
    }
    let runtime = runtimes::resolve(resolved, Role::Runtime, &mut |message| {
        ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
    })?;
    Ok(runtime.environment(env))
}

pub(crate) fn exec_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
    yes: bool,
) -> Result<()> {
    let resolved = load_config(cwd)?;

    // uf's own packages first, and they are the only path that renders
    // anything: every other one hands stdout to a child process, and a banner
    // printed above somebody else's output is uf writing on a report it did
    // not produce. `uf run` has held that line since it was written.
    if exec_uniflowed_virtual_package(cwd, ui, package, args)? {
        return Ok(());
    }

    // And the environment below that branch, not above it. A virtual package
    // resolves its own — `uf exec @uniflowed/test` reaches `test::test`, whose
    // mode is `test` — so loading the `development` cascade first only added a
    // way to fail: a `.env.development` that does not parse would have stopped
    // a command that was never going to read it.
    //
    // Below this line it is the same environment `uf run` gives a task: `ufx`
    // runs a tool against this project, and a codegen that reads
    // `DATABASE_URL` should read the project's.
    let env = runtime_environment(&resolved, ui, project_env(&resolved, None, DEVELOPMENT)?)?;

    if let Some(binary) = installed_binary(&resolved.root, package) {
        return spawn_executable(&resolved.root, ui, &env, &binary, args, package);
    }

    // A path, executed as written. `ufx ./scripts/codegen.js` is a thing
    // people do, and it is not a package name.
    let candidate = Utf8PathBuf::from(package);
    let executable = if candidate.is_absolute() {
        candidate
    } else {
        resolved.root.join(candidate)
    };
    if executable.is_file() {
        return spawn_executable(&resolved.root, ui, &env, &executable, args, package);
    }

    // With the config, like every other command that runs a manager:
    // `packageManager: "pnpm@10"` means `pnpm dlx` even beside a
    // package-lock.json. See ubugeeei-prod/uf#940.
    let detection = detect_package_manager_with(
        &resolved.root,
        &DetectionOptions::from_config(&resolved.config),
    );
    let manager = fetchable(detection.package_manager);
    // Every manager has a fetch-and-run, which is what `fetchable` guarantees:
    // it maps a manager without one onto the one uf would use instead.
    let mut invocation = command_for(manager, Operation::DlxExec)
        .expect("every fetchable manager has a fetch-and-run command");
    invocation.args.push(Cow::Owned(package.to_owned()));
    invocation
        .args
        .extend(args.iter().map(|arg| Cow::Owned(arg.clone())));

    if !yes {
        // Naming the exact command it would otherwise run, so the decision is
        // made by reading rather than by trusting. `uf add` is not a command
        // uf has, so the hint says what a person can actually type.
        bail!(
            "{package} is not installed in this project, and fetching it would run code \
             {lockfile} does not pin.\n\
             Declare it in package.json and run `uf install`, or say so explicitly:\n\
             `uf exec --yes {package}`, which runs `{invocation}`.",
            lockfile = resolved.config.pm.lockfile,
        );
    }

    // The release `packageManager` pins, from the store, with the one `runtime`
    // pins behind it — in that order, so a pinned npm is not shadowed by the
    // npm inside a pinned Node. After the consent check, so a refusal installs
    // nothing.
    let path =
        crate::commands::runtimes::manager_path(&resolved, manager, false, &mut |message| {
            ui.render_err(|renderer, out| renderer.status(out, Status::Info, message));
        })?;
    let env = if path.is_empty() {
        env
    } else {
        path.into_iter().fold(
            project_env(&resolved, None, DEVELOPMENT)?,
            |env, directory| env.with_path_prefix(directory),
        )
    };

    // On stderr, so the fetched binary still owns stdout. Printed rather than
    // silent because "uf downloaded and ran something" is not a thing a person
    // should have to infer from a network light.
    let announcement = format!("fetching and running {package} with `{invocation}`");
    ui.render_err(|renderer, out| {
        renderer.status(out, Status::Info, &announcement);
    });

    let mut fetch = ProcessCommand::new(invocation.program);
    env.apply(&mut fetch);
    let status = fetch
        .args(invocation.args.iter().map(AsRef::as_ref))
        .current_dir(resolved.root.as_std_path())
        .status()
        .with_context(|| format!("failed to run `{invocation}`"))?;
    if !status.success() {
        adopt_exit_status(ui, status, package);
    }
    Ok(())
}

/// Whose rules a `node_modules/.bin` entry is resolved under.
///
/// A parameter rather than a `#[cfg]`, and that is the point of it.
/// ubugeeei-prod/uf#390 argued — correctly — that a `#[cfg(windows)]` branch
/// added today would "compile nowhere and run nowhere": uf publishes no
/// Windows artifact (#309) and every job in `ci.yml` is on Ubuntu. That is
/// true of a *branch*. It is not true of a function whose platform is an
/// argument: [`bin_candidates`] and [`installed_binary_in`] compile and run on
/// the Linux runner for both values, so the Windows decision is exercised on
/// every push rather than trusted.
///
/// What that arrangement still cannot prove is in `tests.rs`, on the tests
/// themselves: nothing here spawns a process, so nothing here shows that
/// `CreateProcess` accepts the file that was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinPlatform {
    /// One file, with the executable bit on it: `node_modules/.bin/<name>`.
    Unix,
    /// A list of extensions, and the extensionless file is not on it.
    Windows,
}

impl BinPlatform {
    /// The rules uf is actually running under.
    const HOST: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Unix
    };
}

/// The extensions Windows will start a `.bin` entry under, in `PATHEXT` order.
///
/// npm, pnpm and yarn each write three files for one executable — `<name>`, an
/// `sh` script for Git Bash and Cygwin; `<name>.cmd`, which is what `cmd.exe`
/// runs; and `<name>.ps1` — and a package may ship a native `.exe` in there
/// instead. So resolution is a list and not a suffix.
///
/// `.ps1` is deliberately absent: it is not in the default `PATHEXT` and
/// `CreateProcess` cannot start a PowerShell script, so the third file a
/// package manager writes is not a candidate for uf any more than the first
/// one is.
///
/// The list is fixed rather than read from `%PATHEXT%`. A machine that has
/// reordered its `PATHEXT` would have uf try `.exe` before `.cmd` where it
/// wanted the reverse — and the alternative is letting an environment variable
/// decide which of two files in `node_modules/.bin` uf executes, which is a
/// worse thing to be able to do to somebody than an unusual ordering.
const WINDOWS_BIN_EXTENSIONS: [&str; 4] = [".com", ".exe", ".bat", ".cmd"];

/// The file names one linked binary can have, in the order to try them.
///
/// On Windows the extensionless file is *skipped* rather than preferred. It is
/// the `sh` script, it exists, and `is_file()` says so — which is why #390 was
/// a confusing "failed to execute" naming a file that plainly exists rather
/// than a lookup that failed.
fn bin_candidates(name: &str, platform: BinPlatform) -> Vec<String> {
    match platform {
        BinPlatform::Unix => vec![name.to_owned()],
        BinPlatform::Windows => WINDOWS_BIN_EXTENSIONS
            .iter()
            .map(|extension| format!("{name}{extension}"))
            .collect(),
    }
}

/// The binary name `package` is linked under, when it is a name at all.
///
/// A scoped name is linked under its bare binary name — `@scope/thing`
/// installs `thing` — which is why the last segment is what is looked up.
///
/// [`None`] for anything that would stop being a single file name. Nothing
/// good comes of joining a caller's string onto a path when it can climb out
/// of it, and `uf exec ../../evil` must not become a lookup in somebody else's
/// `node_modules`.
fn binary_name(package: &str) -> Option<&str> {
    let name = package.rsplit('/').next().unwrap_or(package);
    if name.is_empty() || name.contains(std::path::is_separator) || name.starts_with('.') {
        return None;
    }
    Some(name)
}

/// The project's installed binary for `package`, when there is one.
///
/// `node_modules/.bin` is where every package manager links a dependency's
/// executables, so this is the same lookup `npm exec` does before it considers
/// fetching anything.
///
/// # Arguments, and the CVE the resolution has to respect
///
/// What comes back may be a `.cmd` or a `.bat`, and since Rust 1.77
/// (CVE-2024-24576) [`ProcessCommand`] spawns one of those through `cmd.exe`
/// with batch-specific escaping. `uf exec` forwards arbitrary user arguments —
/// `ufx eslint --fix "src/**/*.js"` — straight into that.
///
/// uf's part of the contract is to add nothing of its own: [`spawn_executable`]
/// passes each argument through `Command::args`, one `argv` entry each, and
/// never `raw_arg`, which is the documented way to *opt out* of that escaping.
/// So a quote, a caret or a percent sign in an argument is the standard
/// library's problem to encode and not uf's to quote — and quoting it here
/// would be uf escaping a string that is about to be escaped again.
pub(crate) fn installed_binary(root: &Utf8Path, package: &str) -> Option<Utf8PathBuf> {
    installed_binary_in(root, package, BinPlatform::HOST)
}

/// [`installed_binary`], with the platform said out loud.
fn installed_binary_in(
    root: &Utf8Path,
    package: &str,
    platform: BinPlatform,
) -> Option<Utf8PathBuf> {
    let name = binary_name(package)?;
    let bin = root.join("node_modules/.bin");
    bin_candidates(name, platform)
        .into_iter()
        .map(|candidate| bin.join(candidate))
        .find(|candidate| candidate.is_file())
}

/// Run one executable, forwarding its arguments and its exit status.
fn spawn_executable(
    root: &Utf8Path,
    ui: &mut Ui,
    env: &ProjectEnv,
    executable: &Utf8Path,
    args: &[String],
    package: &str,
) -> Result<()> {
    let mut process = ProcessCommand::new(executable.as_std_path());
    env.apply(&mut process);
    let status = process
        .args(args)
        .current_dir(root.as_std_path())
        .status()
        .with_context(|| format!("failed to execute {executable}"))?;
    if !status.success() {
        adopt_exit_status(ui, status, package);
    }
    Ok(())
}

/// Report a child's failure and leave with the child's own exit code.
///
/// Not a `bail!`. An error out of `run` becomes [`ExitCode::FAILURE`], which is
/// `1`, so every code a child could exit with — `jest`'s, `eslint`'s, a
/// codegen script's `42` — arrived at the caller as the same number, and a
/// script branching on it could tell nothing apart. `docs/app/reference/cli`
/// has said "exiting with its status" since this command started running
/// anything at all, and #353 said "exit status adopted"; neither was true of
/// any of the three paths that spawn.
///
/// `std::process::exit`, because there is nowhere else to put a number:
/// `run` answers `Result<()>` for every command and `main` turns that into one
/// of two codes. `uf env exec` reached the same conclusion and makes the same
/// call, and its comment says the rest. Nothing is buffered past this point —
/// `Ui` writes and flushes inside `render_err` — so there is nothing for the
/// skipped destructors to lose.
///
/// A child killed by a signal has no code of its own, and answers `1` here.
/// That loses which signal it was, and matching `uf env exec` is worth more
/// than fixing it in one of the two commands: a sibling pair that disagreed
/// about the same event would be the harder thing to reason about.
pub(crate) fn adopt_exit_status(ui: &mut Ui, status: std::process::ExitStatus, package: &str) -> ! {
    // Said before leaving, because the number alone does not say whose it is:
    // a reader looking at `42` should not have to guess whether uf failed or
    // the thing uf ran did.
    ui.error(&anyhow::anyhow!("{package} exited with {status}"));
    std::process::exit(status.code().unwrap_or(1));
}

/// The manager that can fetch and run a package that is not installed.
///
/// [`PackageManager::Uf`] is what detection reports both when a project pins
/// uf and when it shows no evidence of any manager, and uf's own resolver
/// cannot fetch yet — the same reason [`uf_pm::run_install`] substitutes npm,
/// and the same substitution. `npx` comes with Node.js, which a uf project
/// needs anyway.
pub(crate) fn fetchable(manager: PackageManager) -> PackageManager {
    match manager {
        PackageManager::Uf => PackageManager::Npm,
        other => other,
    }
}

/// Packages `uf` implements itself, rather than fetching from a registry.
///
/// The banner is here rather than in [`exec_package`] because this is the only
/// path where uf is the thing that runs: every other one spawns a process and
/// gives it stdout, and a heading printed above another program's output is uf
/// signing a report it did not write.
fn exec_uniflowed_virtual_package(
    cwd: &Utf8Path,
    ui: &mut Ui,
    package: &str,
    args: &[String],
) -> Result<bool> {
    if !matches!(
        package,
        "@uniflowed/create"
            | "uf/create"
            | "@uniflowed/test"
            | "uf/test"
            | "@uniflowed/pm"
            | "uf/pm"
    ) {
        return Ok(false);
    }

    ui.render(|renderer, out| {
        renderer.banner(out, "ufx", Some(package));
    });

    match package {
        "@uniflowed/create" | "uf/create" => {
            let Some(kind) = args.first().map(String::as_str) else {
                bail!("ufx {package} requires app or lib");
            };
            match kind {
                "app" => {
                    // The same two positionals `uf create app` takes, handed
                    // over unresolved: `create::app_arguments` decides which
                    // of them is a template, so the two front doors cannot
                    // read the same command differently. They did — this one
                    // already accepted `ufx @uniflowed/create app my-site`
                    // while `uf create app my-site` was rejected (#322).
                    create::create(
                        cwd,
                        ui,
                        CreateCommand::App {
                            template_or_path: args.get(1).cloned(),
                            path: args.get(2).map(Utf8PathBuf::from),
                            name: None,
                            force: args.iter().any(|arg| arg == "--force"),
                        },
                    )?;
                }
                "lib" => {
                    create::create(
                        cwd,
                        ui,
                        CreateCommand::Lib {
                            path: args.get(1).map(Utf8PathBuf::from),
                            name: None,
                            force: args.iter().any(|arg| arg == "--force"),
                        },
                    )?;
                }
                other => bail!("unknown @uniflowed/create target {other:?}"),
            }
            Ok(true)
        }
        "@uniflowed/test" | "uf/test" => {
            test::test(
                cwd,
                ui,
                test::TestArgs {
                    list: args.iter().any(|arg| arg == "--list"),
                    watch: args.iter().any(|arg| arg == "--watch"),
                    ..test::TestArgs::default()
                },
            )?;
            Ok(true)
        }
        "@uniflowed/pm" | "uf/pm" => {
            // `ufx @uniflowed/pm` is the package, not the flag surface: the
            // frozen install is `uf install --frozen-lockfile`.
            pm::install(cwd, ui, false, false)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests;
