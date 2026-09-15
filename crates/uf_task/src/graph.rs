//! The tasks one `uf run` will execute, and what has to happen before what.
//!
//! `run_named_task` used to be a recursion over `dependsOn` with a visited
//! set, which is a depth-first walk that happens to also be the schedule. Two
//! things that costs: two dependencies with no path between them are run one
//! after the other for no reason, and a cycle is not an error — the visited
//! set swallows it and the second visit silently does nothing, so `a` depending
//! on `b` depending on `a` "succeeds" having run `b` and not `a`.
//!
//! So the walk builds a graph first and the schedule reads it.
//!
//! # Across a workspace
//!
//! A plan is over *packages* as well as tasks: the project `uf run` started in,
//! and the members of its workspace, each with its own `uf.config.js`. A node is
//! one task in one package, and there are three ways for one to reach another:
//!
//! * `dependsOn: ["lint"]` names a task in the same package, as it always has.
//! * `dependsOn: ["ui#build"]` names `build` in the member `ui`. A name the
//!   package itself defines, written exactly so, wins: a task that is really
//!   called `build#docs` keeps meaning what it meant before `#` meant anything.
//! * A task asked for in several packages at once — `uf run build -r` — waits
//!   for the same task in the packages its own `package.json` depends on, which
//!   is the order a workspace has to be built in. Only the packages that were
//!   asked are waited for: `--filter app` runs `app`'s `build` alone, and
//!   `--filter app...` is how its dependencies are asked for too.
//!
//! The last two cross a package boundary, and a node says which of its
//! dependencies do — [`PlanNode::across`] — because the runner keys a task on
//! the results it reaches in other packages. See `runner` for why that is not
//! also true inside one package.

use std::collections::BTreeMap;

use compact_str::{CompactString, format_compact};
use uf_config::{TaskDefinition, UniflowedConfig};

/// One project a plan can reach tasks in.
#[derive(Debug, Clone, Copy)]
pub struct PlanPackage<'a> {
    /// What `pkg#task` calls it — empty for a package nothing names, which is
    /// what the project `uf run` started in is when it is nobody's member.
    pub name: &'a str,
    /// Where it is, relative to the workspace root. `pkg#task` accepts this
    /// too, for the two members that share a name.
    pub path: &'a str,
    /// Its `tasks`.
    pub tasks: &'a BTreeMap<CompactString, TaskDefinition>,
    /// Indices of the packages its `package.json` depends on.
    pub dependencies: &'a [usize],
}

/// One task in a plan.
#[derive(Debug, Clone)]
pub struct PlanNode {
    /// Index of the package it belongs to.
    pub package: usize,
    /// The task's name in that package's `uf.config.js`.
    pub name: CompactString,
    /// What a reader calls it: the name, or `package#name` in a package that
    /// has a name, so two packages' `build` are never one line in a report.
    pub label: CompactString,
    /// Indices in [`Plan::nodes`] that must finish before this one starts.
    pub dependencies: Vec<usize>,
    /// Those of [`Self::dependencies`] that belong to another package.
    pub across: Vec<usize>,
}

/// What a `uf run` will execute, in dependency order.
///
/// Every node is something a requested task reaches, and every node comes
/// after everything it depends on — so with one request, that request is the
/// last node.
#[derive(Debug, Clone)]
pub struct Plan {
    nodes: Vec<PlanNode>,
    requested: Vec<usize>,
}

/// Why a plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A name `uf.config.js` does not define. Carries the name — as a reader
    /// would write it, so `ui#build` for another package's — and the chain
    /// that asked for it, empty when the caller asked for it directly.
    Unknown {
        name: String,
        through: Vec<CompactString>,
    },
    /// `pkg#task` names a package the workspace does not have. Carries the
    /// reference as written, and the chain that asked for it.
    UnknownPackage {
        reference: String,
        through: Vec<CompactString>,
    },
    /// `dependsOn` closes a loop. Carries it, starting and ending at the same
    /// name.
    Cycle(Vec<CompactString>),
}

impl Plan {
    /// The plan for running `requested` in a project with no workspace.
    ///
    /// # Errors
    ///
    /// When a name is not defined, or `dependsOn` closes a loop.
    pub fn build(config: &UniflowedConfig, requested: &str) -> Result<Self, PlanError> {
        let package = PlanPackage {
            name: "",
            path: "",
            tasks: &config.tasks,
            dependencies: &[],
        };
        Self::build_workspace(&[package], &[(0, requested)])
    }

    /// The plan for running every one of `requests` — a package index and a
    /// task name — and everything they reach.
    ///
    /// # Errors
    ///
    /// When a name is not defined, a `pkg#task` names no package, or
    /// `dependsOn` and the packages' dependencies close a loop between them.
    pub fn build_workspace(
        packages: &[PlanPackage<'_>],
        requests: &[(usize, &str)],
    ) -> Result<Self, PlanError> {
        let mut builder = Builder {
            packages,
            requests,
            index: BTreeMap::new(),
            nodes: Vec::new(),
            stack: Vec::new(),
        };
        let mut requested = Vec::with_capacity(requests.len());
        for &(package, name) in requests {
            let at = builder.visit(package, name, &[])?;
            push_unique(&mut requested, at);
        }
        Ok(Self {
            nodes: builder.nodes,
            requested,
        })
    }

    /// Every task to run, dependencies before dependents.
    #[must_use]
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    /// The nodes that were asked for, in the order they were asked for.
    #[must_use]
    pub fn requested(&self) -> &[usize] {
        &self.requested
    }

    /// How many tasks are in it.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether it is empty, which it never is: a plan always holds at least
    /// the task that was asked for.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

struct Builder<'a, 'b> {
    packages: &'b [PlanPackage<'a>],
    requests: &'b [(usize, &'b str)],
    index: BTreeMap<(usize, CompactString), usize>,
    nodes: Vec<PlanNode>,
    stack: Vec<(usize, CompactString)>,
}

impl Builder<'_, '_> {
    fn visit(
        &mut self,
        package: usize,
        name: &str,
        through: &[CompactString],
    ) -> Result<usize, PlanError> {
        let key = (package, CompactString::new(name));
        if let Some(&at) = self.index.get(&key) {
            return Ok(at);
        }
        if let Some(from) = self.stack.iter().position(|held| *held == key) {
            let mut cycle: Vec<CompactString> = self.stack[from..]
                .iter()
                .map(|(owner, task)| self.label(*owner, task))
                .collect();
            cycle.push(self.label(package, name));
            return Err(PlanError::Cycle(cycle));
        }
        // Copied out of `self` so the task borrows the configuration rather
        // than the builder, which the visits below need mutably.
        let tasks = self.packages[package].tasks;
        let Some(task) = tasks.get(name) else {
            return Err(PlanError::Unknown {
                name: self.label(package, name).to_string(),
                through: through.to_vec(),
            });
        };

        self.stack.push(key.clone());
        let chain: Vec<CompactString> = self
            .stack
            .iter()
            .map(|(owner, task)| self.label(*owner, task))
            .collect();
        let mut dependencies = Vec::with_capacity(task.depends_on().len());
        let mut across = Vec::new();
        for written in task.depends_on() {
            let (target, target_name) = self.resolve(package, written.as_str(), &chain)?;
            let at = self.visit(target, target_name, &chain)?;
            // A name written twice in one `dependsOn` is one edge.
            push_unique(&mut dependencies, at);
            if target != package {
                push_unique(&mut across, at);
            }
        }
        if self
            .requests
            .iter()
            .any(|&(asked, task)| asked == package && task == name)
        {
            for target in self.requested_upstream(package, name) {
                let at = self.visit(target, name, &chain)?;
                push_unique(&mut dependencies, at);
                push_unique(&mut across, at);
            }
        }
        self.stack.pop();

        let at = self.nodes.len();
        self.nodes.push(PlanNode {
            package,
            name: key.1.clone(),
            label: self.label(package, name),
            dependencies,
            across,
        });
        self.index.insert(key, at);
        Ok(at)
    }

    /// The package and task one `dependsOn` entry of `package` names.
    ///
    /// The package's own task first, as written, and `pkg#task` only when there
    /// is no such task — so no config that worked before `#` meant anything
    /// means something else now. A name with no `#` that is not defined is left
    /// for [`Self::visit`] to report, which is where that error has always come
    /// from.
    fn resolve<'w>(
        &self,
        package: usize,
        written: &'w str,
        through: &[CompactString],
    ) -> Result<(usize, &'w str), PlanError> {
        if self.packages[package].tasks.contains_key(written) {
            return Ok((package, written));
        }
        let Some((owner, name)) = written.split_once('#') else {
            return Ok((package, written));
        };
        self.packages
            .iter()
            .position(|candidate| {
                !owner.is_empty() && (candidate.name == owner || candidate.path == owner)
            })
            .map(|target| (target, name))
            .ok_or_else(|| PlanError::UnknownPackage {
                reference: written.to_owned(),
                through: through.to_vec(),
            })
    }

    /// The nearest packages `package` depends on, directly or through packages
    /// that were not asked, where `name` was also asked for.
    ///
    /// Nearest, because each of those waits for its own in turn: `app` on `ui`
    /// on `utils` is two edges rather than three. Through a package that was
    /// not asked, because one without a `build` of its own still sits between
    /// two that have one, and the order between those two is still real.
    fn requested_upstream(&self, package: usize, name: &str) -> Vec<usize> {
        let mut found = Vec::new();
        let mut seen = vec![false; self.packages.len()];
        let mut pending: Vec<usize> = self.packages[package].dependencies.to_vec();
        while let Some(next) = pending.pop() {
            if next == package || next >= seen.len() || std::mem::replace(&mut seen[next], true) {
                continue;
            }
            if self
                .requests
                .iter()
                .any(|&(asked, task)| asked == next && task == name)
            {
                found.push(next);
            } else {
                pending.extend_from_slice(self.packages[next].dependencies);
            }
        }
        found.sort_unstable();
        found
    }

    fn label(&self, package: usize, name: &str) -> CompactString {
        match self.packages[package].name {
            "" => CompactString::new(name),
            owner => format_compact!("{owner}#{name}"),
        }
    }
}

fn push_unique(into: &mut Vec<usize>, at: usize) {
    if !into.contains(&at) {
        into.push(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tasks(tasks: &[(&str, &[&str])]) -> BTreeMap<CompactString, TaskDefinition> {
        let mut map = BTreeMap::new();
        for (name, dependencies) in tasks {
            let task = serde_json::json!({ "command": "true", "dependsOn": dependencies });
            map.insert(
                CompactString::new(name),
                serde_json::from_value(task).unwrap(),
            );
        }
        map
    }

    fn config(list: &[(&str, &[&str])]) -> UniflowedConfig {
        let mut config = UniflowedConfig::default();
        config.tasks = tasks(list);
        config
    }

    fn names(plan: &Plan) -> Vec<String> {
        plan.nodes()
            .iter()
            .map(|node| node.label.to_string())
            .collect()
    }

    #[test]
    fn dependencies_come_before_the_task_that_asked_for_them() {
        let config = config(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
        let plan = Plan::build(&config, "c").unwrap();
        assert_eq!(names(&plan), vec!["a", "b", "c"]);
        assert_eq!(plan.requested(), &[2]);
    }

    #[test]
    fn a_task_two_dependents_share_is_one_node() {
        let config = config(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
        let plan = Plan::build(&config, "d").unwrap();
        assert_eq!(names(&plan), vec!["a", "b", "c", "d"]);
        assert_eq!(plan.nodes()[3].dependencies, vec![1, 2]);
        assert!(
            plan.nodes()[3].across.is_empty(),
            "one package, no crossing"
        );
    }

    /// The visited set used to make this succeed while running half of it.
    #[test]
    fn a_cycle_is_an_error_rather_than_a_task_quietly_skipped() {
        let config = config(&[("a", &["b"]), ("b", &["a"])]);
        let error = Plan::build(&config, "a").unwrap_err();
        assert_eq!(
            error,
            PlanError::Cycle(vec![
                CompactString::new("a"),
                CompactString::new("b"),
                CompactString::new("a"),
            ])
        );
    }

    #[test]
    fn a_task_that_depends_on_itself_is_a_cycle() {
        let config = config(&[("a", &["a"])]);
        assert!(matches!(
            Plan::build(&config, "a"),
            Err(PlanError::Cycle(_))
        ));
    }

    #[test]
    fn an_undefined_dependency_names_who_asked_for_it() {
        let config = config(&[("a", &["missing"])]);
        let error = Plan::build(&config, "a").unwrap_err();
        assert_eq!(
            error,
            PlanError::Unknown {
                name: String::from("missing"),
                through: vec![CompactString::new("a")],
            }
        );
    }

    #[test]
    fn an_undefined_request_carries_no_chain() {
        let config = config(&[("a", &[])]);
        assert_eq!(
            Plan::build(&config, "b").unwrap_err(),
            PlanError::Unknown {
                name: String::from("b"),
                through: Vec::new(),
            }
        );
    }

    // --- across packages ----------------------------------------------------

    /// `utils` ← `ui` ← `app`, by `package.json`, each with a `build`.
    fn chain() -> [BTreeMap<CompactString, TaskDefinition>; 3] {
        [
            tasks(&[("build", &[])]),
            tasks(&[("build", &[])]),
            tasks(&[("build", &[]), ("lint", &[])]),
        ]
    }

    fn packages<'a>(
        tasks: &'a [BTreeMap<CompactString, TaskDefinition>],
        dependencies: &'a [Vec<usize>],
    ) -> Vec<PlanPackage<'a>> {
        let names = ["utils", "ui", "app", "mid"];
        tasks
            .iter()
            .zip(dependencies)
            .enumerate()
            .map(|(at, (tasks, dependencies))| PlanPackage {
                name: names[at],
                path: ["packages/utils", "packages/ui", "packages/app", "mid"][at],
                tasks,
                dependencies,
            })
            .collect()
    }

    #[test]
    fn a_task_asked_for_in_every_package_runs_in_the_order_they_depend_on_each_other() {
        let tasks = chain();
        let dependencies = [vec![], vec![0], vec![1]];
        let packages = packages(&tasks, &dependencies);

        // Asked in the opposite order, which is the order a reader would not
        // want them run in.
        let plan =
            Plan::build_workspace(&packages, &[(2, "build"), (1, "build"), (0, "build")]).unwrap();

        assert_eq!(names(&plan), vec!["utils#build", "ui#build", "app#build"]);
        assert_eq!(plan.nodes()[2].dependencies, vec![1]);
        assert_eq!(
            plan.nodes()[2].across,
            vec![1],
            "app waits for ui, not utils"
        );
        assert_eq!(plan.requested(), &[2, 1, 0]);
    }

    #[test]
    fn only_the_packages_that_were_asked_are_waited_for() {
        let tasks = chain();
        let dependencies = [vec![], vec![0], vec![1]];
        let packages = packages(&tasks, &dependencies);

        let plan = Plan::build_workspace(&packages, &[(2, "build")]).unwrap();

        assert_eq!(names(&plan), vec!["app#build"]);
    }

    /// `app` → `mid` → `utils`, where `mid` has no `build`: `app`'s still waits
    /// for `utils`'s, because `mid` sitting between them does not make the
    /// order between them any less real.
    #[test]
    fn a_package_without_the_task_is_looked_through() {
        let tasks = [
            tasks(&[("build", &[])]),
            BTreeMap::new(),
            tasks(&[("build", &[])]),
            BTreeMap::new(),
        ];
        let dependencies = [vec![], vec![], vec![3], vec![0]];
        let packages = packages(&tasks, &dependencies);

        let plan = Plan::build_workspace(&packages, &[(2, "build"), (0, "build")]).unwrap();

        assert_eq!(names(&plan), vec!["utils#build", "app#build"]);
    }

    #[test]
    fn a_hash_in_depends_on_names_a_task_in_another_package() {
        let tasks = [
            tasks(&[("build", &[])]),
            tasks(&[("build", &["utils#build"])]),
        ];
        let dependencies = [vec![], vec![]];
        let packages = packages(&tasks, &dependencies);

        let plan = Plan::build_workspace(&packages, &[(1, "build")]).unwrap();

        assert_eq!(names(&plan), vec!["utils#build", "ui#build"]);
        assert_eq!(plan.nodes()[1].across, vec![0]);
    }

    #[test]
    fn a_package_may_be_named_by_its_path() {
        let tasks = [
            tasks(&[("build", &[])]),
            tasks(&[("build", &["packages/utils#build"])]),
        ];
        let dependencies = [vec![], vec![]];
        let packages = packages(&tasks, &dependencies);

        let plan = Plan::build_workspace(&packages, &[(1, "build")]).unwrap();

        assert_eq!(names(&plan), vec!["utils#build", "ui#build"]);
    }

    /// A task that is really called `docs#build` is still that task.
    #[test]
    fn a_task_named_with_a_hash_in_its_own_package_wins() {
        let config = config(&[("docs#build", &[]), ("ci", &["docs#build"])]);
        let plan = Plan::build(&config, "ci").unwrap();
        assert_eq!(names(&plan), vec!["docs#build", "ci"]);
        assert!(plan.nodes()[1].across.is_empty());
    }

    #[test]
    fn a_package_the_workspace_does_not_have_is_named_with_who_asked() {
        let config = config(&[("ci", &["site#build"])]);
        assert_eq!(
            Plan::build(&config, "ci").unwrap_err(),
            PlanError::UnknownPackage {
                reference: String::from("site#build"),
                through: vec![CompactString::new("ci")],
            }
        );
    }

    #[test]
    fn a_task_another_package_does_not_define_is_named_as_written() {
        let tasks = [tasks(&[]), tasks(&[("build", &["utils#build"])])];
        let dependencies = [vec![], vec![]];
        let packages = packages(&tasks, &dependencies);

        assert_eq!(
            Plan::build_workspace(&packages, &[(1, "build")]).unwrap_err(),
            PlanError::Unknown {
                name: String::from("utils#build"),
                through: vec![CompactString::new("ui#build")],
            }
        );
    }

    #[test]
    fn packages_that_depend_on_each_other_are_a_cycle_when_both_are_asked() {
        let tasks = [tasks(&[("build", &[])]), tasks(&[("build", &[])])];
        let dependencies = [vec![1], vec![0]];
        let packages = packages(&tasks, &dependencies);

        let error = Plan::build_workspace(&packages, &[(0, "build"), (1, "build")]).unwrap_err();

        assert_eq!(
            error,
            PlanError::Cycle(vec![
                CompactString::new("utils#build"),
                CompactString::new("ui#build"),
                CompactString::new("utils#build"),
            ])
        );
    }
}
