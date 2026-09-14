//! What a task is given besides its command and its inputs.
//!
//! A task's answer can depend on a variable, so every variable uf sets for it
//! is part of its key. The key is not all that is written down: the note
//! `--why` compares against keeps what the last run keyed on, so that a miss
//! can be explained by naming what changed. Until #1006 the note did that for
//! the environment by keeping it — the mode and every `.env` value, verbatim,
//! under `.uf/cache`, which is exactly what a CI step that caches the task
//! cache archives, and the `.env` cascade is where credentials are kept.
//!
//! So no value is kept. [`Environment::file`] and [`Environment::task`] digest
//! a value together with its name and where it came from, and hold only the
//! digest. The name stays readable, which is what lets `--why` say that
//! `API_TOKEN` changed without saying what to. Names are not secret — `uf
//! inspect` prints them — and neither are the mode and the directory.
//!
//! What a digest does not hide is a value somebody can guess: `DEBUG=true` is
//! one of a handful of strings, and anybody holding the note can try each of
//! them. The key has had the same property all along, since everything else it
//! is built from is in the note or in the repository — and a key that only a
//! secret could reproduce is a key the CI runner a cache was restored onto
//! never hits. What this closes is reading a credential off a disk, not
//! guessing one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cache::{Change, Difference, first_difference};
use crate::digest::{Fields, hex};

/// The environment uf gives one task, with every value reduced to a digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    /// The mode the project's `.env` files were selected for.
    mode: String,
    /// The directory the task runs in, as `uf.config.js` writes it, when it
    /// names one.
    directory: Option<String>,
    /// Every variable uf sets, by name, as a digest of where it came from, its
    /// name and its value.
    variables: BTreeMap<String, String>,
}

impl Environment {
    /// An environment selected for `mode`, with nothing in it yet.
    #[must_use]
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.to_owned(),
            directory: None,
            variables: BTreeMap::new(),
        }
    }

    /// A variable from the project's `.env` files.
    pub fn file(&mut self, name: &str, value: &str) -> &mut Self {
        self.set("file", name, value)
    }

    /// A variable from the task's own `env`.
    ///
    /// It replaces a file's value of the same name, because that is what the
    /// task is given. It does not digest like the same value read from a file:
    /// a nested uf treats the two differently — a file's value is marked as one
    /// its own files may overrule — so the two are different environments.
    pub fn task(&mut self, name: &str, value: &str) -> &mut Self {
        self.set("task", name, value)
    }

    /// The directory the task runs in.
    pub fn directory(&mut self, directory: &str) -> &mut Self {
        self.directory = Some(directory.to_owned());
        self
    }

    fn set(&mut self, origin: &str, name: &str, value: &str) -> &mut Self {
        let mut fields = Fields::new("uf task variable v1");
        fields.push(origin).push(name).push(value);
        self.variables
            .insert(name.to_owned(), hex(&fields.finish()));
        self
    }

    /// Add this environment to a key.
    pub(crate) fn push_into(&self, fields: &mut Fields) {
        fields.push(&self.mode);
        match &self.directory {
            Some(directory) => fields.push("directory").push(directory),
            None => fields.push("root"),
        };
        // The count first, so the variables cannot be read as running on
        // into whatever the key adds after them.
        fields.push(&self.variables.len().to_string());
        for (name, digest) in &self.variables {
            fields.push(name).push(digest);
        }
    }

    /// The first difference between the environment a note recorded and
    /// `now`, or [`None`] when they are the same.
    pub(crate) fn diff(&self, now: &Self) -> Option<Change> {
        if self.mode != now.mode {
            return Some(Change::Mode);
        }
        if self.directory != now.directory {
            return Some(Change::Directory);
        }
        let difference = first_difference(named(&self.variables), named(&now.variables))?;
        Some(match difference {
            Difference::Removed(name) => Change::VariableRemoved(name.to_owned()),
            Difference::Added(name) => Change::VariableAdded(name.to_owned()),
            Difference::Changed(name) => Change::VariableChanged(name.to_owned()),
        })
    }
}

/// Variables as `(name, digest)`, in name order.
fn named(variables: &BTreeMap<String, String>) -> impl Iterator<Item = (&str, &str)> {
    variables
        .iter()
        .map(|(name, digest)| (name.as_str(), digest.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "not-a-real-secret-4f1c2b";

    #[test]
    fn a_value_is_not_in_what_is_written_down() {
        let mut environment = Environment::new("development");
        environment.file("API_TOKEN", SECRET).task("OTHER", SECRET);
        let written = serde_json::to_string(&environment).unwrap();
        assert!(written.contains("API_TOKEN"), "{written}");
        assert!(!written.contains(SECRET), "{written}");
    }

    #[test]
    fn the_variable_that_changed_is_named() {
        let mut before = Environment::new("development");
        before.file("A", "1").file("B", "2");
        let mut after = Environment::new("development");
        after.file("A", "1").file("B", "3");
        let mut fewer = Environment::new("development");
        fewer.file("A", "1");

        assert_eq!(before.diff(&before.clone()), None);
        assert_eq!(
            before.diff(&after),
            Some(Change::VariableChanged(String::from("B")))
        );
        assert_eq!(
            before.diff(&fewer),
            Some(Change::VariableRemoved(String::from("B")))
        );
        assert_eq!(
            fewer.diff(&before),
            Some(Change::VariableAdded(String::from("B")))
        );
    }

    #[test]
    fn a_task_value_is_not_the_same_value_read_from_a_file() {
        let mut file = Environment::new("development");
        file.file("A", "1");
        let mut task = Environment::new("development");
        task.task("A", "1");
        assert_eq!(
            file.diff(&task),
            Some(Change::VariableChanged(String::from("A")))
        );
    }
}
