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

use std::collections::BTreeMap;

use compact_str::CompactString;
use uf_config::{TaskDefinition, UniflowedConfig};

/// One task in a plan.
#[derive(Debug, Clone)]
pub struct PlanNode {
    /// The task's name in `uf.config.js`.
    pub name: CompactString,
    /// Indices in [`Plan::nodes`] that must finish before this one starts.
    pub dependencies: Vec<usize>,
}

/// What a `uf run` will execute, in dependency order.
///
/// The requested task is always last, and every other node is something it
/// reaches through `dependsOn`.
#[derive(Debug, Clone)]
pub struct Plan {
    nodes: Vec<PlanNode>,
}

/// Why a plan could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// A name `uf.config.js` does not define. Carries the name, and the chain
    /// that asked for it — empty when the caller asked for it directly.
    Unknown {
        name: String,
        through: Vec<CompactString>,
    },
    /// `dependsOn` closes a loop. Carries it, starting and ending at the same
    /// name.
    Cycle(Vec<CompactString>),
}

impl Plan {
    /// The plan for running `requested`.
    ///
    /// # Errors
    ///
    /// When a name is not defined, or `dependsOn` closes a loop.
    pub fn build(config: &UniflowedConfig, requested: &str) -> Result<Self, PlanError> {
        let mut builder = Builder {
            tasks: &config.tasks,
            index: BTreeMap::new(),
            nodes: Vec::new(),
            stack: Vec::new(),
        };
        builder.visit(requested, &[])?;
        Ok(Self {
            nodes: builder.nodes,
        })
    }

    /// Every task to run, dependencies before dependents.
    #[must_use]
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    /// The requested task's index, which is the last node.
    #[must_use]
    pub fn requested(&self) -> usize {
        self.nodes.len() - 1
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

struct Builder<'a> {
    tasks: &'a BTreeMap<CompactString, TaskDefinition>,
    index: BTreeMap<CompactString, usize>,
    nodes: Vec<PlanNode>,
    stack: Vec<CompactString>,
}

impl Builder<'_> {
    fn visit(&mut self, name: &str, through: &[CompactString]) -> Result<usize, PlanError> {
        if let Some(&at) = self.index.get(name) {
            return Ok(at);
        }
        if let Some(from) = self.stack.iter().position(|held| held == name) {
            let mut cycle: Vec<CompactString> = self.stack[from..].to_vec();
            cycle.push(CompactString::new(name));
            return Err(PlanError::Cycle(cycle));
        }
        let Some(task) = self.tasks.get(name) else {
            return Err(PlanError::Unknown {
                name: name.to_owned(),
                through: through.to_vec(),
            });
        };

        self.stack.push(CompactString::new(name));
        let mut dependencies = Vec::with_capacity(task.depends_on().len());
        for dependency in task.depends_on() {
            let chain = self.stack.clone();
            let at = self.visit(dependency.as_str(), &chain)?;
            // A name written twice in one `dependsOn` is one edge.
            if !dependencies.contains(&at) {
                dependencies.push(at);
            }
        }
        self.stack.pop();

        let at = self.nodes.len();
        self.nodes.push(PlanNode {
            name: CompactString::new(name),
            dependencies,
        });
        self.index.insert(CompactString::new(name), at);
        Ok(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(tasks: &[(&str, &[&str])]) -> UniflowedConfig {
        let mut config = UniflowedConfig::default();
        for (name, dependencies) in tasks {
            let task = serde_json::json!({ "command": "true", "dependsOn": dependencies });
            config.tasks.insert(
                CompactString::new(name),
                serde_json::from_value(task).unwrap(),
            );
        }
        config
    }

    fn names(plan: &Plan) -> Vec<String> {
        plan.nodes()
            .iter()
            .map(|node| node.name.to_string())
            .collect()
    }

    #[test]
    fn dependencies_come_before_the_task_that_asked_for_them() {
        let config = config(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
        let plan = Plan::build(&config, "c").unwrap();
        assert_eq!(names(&plan), vec!["a", "b", "c"]);
        assert_eq!(plan.requested(), 2);
    }

    #[test]
    fn a_task_two_dependents_share_is_one_node() {
        let config = config(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
        let plan = Plan::build(&config, "d").unwrap();
        assert_eq!(names(&plan), vec!["a", "b", "c", "d"]);
        assert_eq!(plan.nodes()[3].dependencies, vec![1, 2]);
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
}
