//! What a task's declared `args` make of the words after its name.
//!
//! `uf run deploy staging` has always appended `staging` to `deploy`'s command.
//! A task that declares its arguments keeps that meaning exactly, and gains
//! three things around it: a name to pass each one by (`--target staging`),
//! the values it may take (anything else is refused by name), and a default
//! for when it is left out. What is still missing after that is the caller's
//! to ask about — `uf run` asks at a terminal and refuses everywhere else —
//! which is why this module stops at [`Given::missing`] rather than deciding.
//!
//! # The rules, in the order they apply
//!
//! 1. `--name value` and `--name=value` fill the argument declared as `name`.
//! 2. Any other word that does not start with `-` fills the next argument not
//!    yet filled, in the order they are declared.
//! 3. Everything else — a flag no argument declares, a word past the last
//!    declared argument — is handed to the command untouched, after the
//!    declared values and in the order it was written. That is what every word
//!    was before `args` existed, so a task that declares none is unchanged.
//!
//! The declared values are appended in declaration order whichever way they
//! were given, so the command sees one order however it was called. An
//! argument that may be left out and has no default therefore has to come
//! after every one that cannot: leaving it out would otherwise move the ones
//! after it into its place. [`check`] refuses that declaration.
//!
//! # Quoting
//!
//! A declared value is one word, whatever it contains — it may have come from
//! a prompt, where a space is an ordinary character — so it is quoted for the
//! command. An undeclared word is appended as it always was, unquoted, because
//! a task that relies on `uf run lint 'src/*.js'` reaching a shell as a glob
//! is relying on that.

use std::borrow::Cow;
use std::fmt;

use uf_config::TaskArgument;

/// Why a task's declared `args` cannot be used as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclarationError {
    /// A name `--name` could not spell.
    BadName(String),
    /// Two arguments with one name.
    Duplicate(String),
    /// A default that is not one of the choices.
    DefaultNotAChoice { name: String, default: String },
    /// `required: true` and a default, which cannot both hold.
    RequiredWithDefault(String),
    /// An argument that may be left out, with no default, before one that
    /// may not.
    OptionalBeforeRequired { optional: String, after: String },
}

impl fmt::Display for DeclarationError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName(name) => write!(
                out,
                "{name:?} cannot be an argument's name: it has to be letters, digits, `-` \
                 and `_`, starting with a letter or digit, so that `--{name}` can pass it"
            ),
            Self::Duplicate(name) => write!(out, "two arguments are called {name:?}"),
            Self::DefaultNotAChoice { name, default } => write!(
                out,
                "<{name}> defaults to {default:?}, which is not one of its choices"
            ),
            Self::RequiredWithDefault(name) => write!(
                out,
                "<{name}> is `required: true` and has a default, so it can never be missing; \
                 drop one of the two"
            ),
            Self::OptionalBeforeRequired { optional, after } => write!(
                out,
                "<{optional}> may be left out and has no default, and <{after}> comes after it; \
                 leaving <{optional}> out would move <{after}>'s value into its place. \
                 Give <{optional}> a default, or declare it last"
            ),
        }
    }
}

/// Whether `declared` is a list of arguments `uf run` can fill.
///
/// # Errors
///
/// The first problem found, in declaration order.
pub fn check(declared: &[TaskArgument]) -> Result<(), DeclarationError> {
    let mut optional: Option<&str> = None;
    for (at, argument) in declared.iter().enumerate() {
        let name = argument.name.as_str();
        if !is_name(name) {
            return Err(DeclarationError::BadName(name.to_owned()));
        }
        if declared[..at].iter().any(|before| before.name == name) {
            return Err(DeclarationError::Duplicate(name.to_owned()));
        }
        if let Some(default) = &argument.default {
            if argument.required == Some(true) {
                return Err(DeclarationError::RequiredWithDefault(name.to_owned()));
            }
            if !argument.choices.is_empty() && !argument.choices.contains(default) {
                return Err(DeclarationError::DefaultNotAChoice {
                    name: name.to_owned(),
                    default: default.to_string(),
                });
            }
        }
        let may_vanish = !argument.is_required() && argument.default.is_none();
        match optional {
            Some(optional) if !may_vanish => {
                return Err(DeclarationError::OptionalBeforeRequired {
                    optional: optional.to_owned(),
                    after: name.to_owned(),
                });
            }
            None if may_vanish => optional = Some(name),
            _ => {}
        }
    }
    Ok(())
}

/// Letters, digits, `-` and `_`, starting with a letter or a digit.
fn is_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

/// Why the words given do not fill the declared arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgumentError {
    /// `--name` at the end, with nothing after it.
    NoValue(String),
    /// One argument given twice.
    Twice {
        name: String,
        first: String,
        second: String,
    },
    /// A value that is not one of the argument's choices.
    NotAChoice {
        name: String,
        value: String,
        choices: Vec<String>,
    },
    /// Required arguments nobody gave, in declaration order.
    Missing(Vec<Missing>),
}

/// One argument that had to be given and was not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// Its name.
    pub name: String,
    /// What it is for, if the task said.
    pub description: Option<String>,
    /// What it may be, or nothing for any value.
    pub choices: Vec<String>,
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoValue(name) => write!(out, "--{name} needs a value after it"),
            Self::Twice {
                name,
                first,
                second,
            } => write!(out, "<{name}> was given twice: {first:?} and {second:?}"),
            Self::NotAChoice {
                name,
                value,
                choices,
            } => write!(
                out,
                "<{name}> cannot be {value:?}; it is one of: {}",
                choices.join(", ")
            ),
            Self::Missing(missing) => {
                let names = missing
                    .iter()
                    .map(|argument| format!("<{}>", argument.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(out, "missing {names}")?;
                for argument in missing {
                    write!(out, "\n  <{}>", argument.name)?;
                    if let Some(description) = &argument.description {
                        write!(out, "  {description}")?;
                    }
                    if !argument.choices.is_empty() {
                        write!(out, " — one of: {}", argument.choices.join(", "))?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// The words after a task's name, sorted into the arguments it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Given<'d> {
    declared: &'d [TaskArgument],
    /// One slot per declared argument, in declaration order.
    values: Vec<Option<String>>,
    /// Everything no argument declares, in the order it was written.
    extra: Vec<String>,
}

impl<'d> Given<'d> {
    /// Sort `words` into `declared`.
    ///
    /// # Errors
    ///
    /// A `--name` with no value, an argument given twice, or a value outside
    /// an argument's choices.
    pub fn parse(declared: &'d [TaskArgument], words: &[String]) -> Result<Self, ArgumentError> {
        let mut given = Self {
            declared,
            values: vec![None; declared.len()],
            extra: Vec::new(),
        };
        let mut words = words.iter();
        while let Some(word) = words.next() {
            if let Some(flag) = word.strip_prefix("--") {
                let (name, inline) = match flag.split_once('=') {
                    Some((name, value)) => (name, Some(value)),
                    None => (flag, None),
                };
                if let Some(at) = declared.iter().position(|argument| argument.name == name) {
                    let value = match inline {
                        Some(value) => value.to_owned(),
                        None => words
                            .next()
                            .cloned()
                            .ok_or_else(|| ArgumentError::NoValue(name.to_owned()))?,
                    };
                    given.fill(at, value)?;
                    continue;
                }
            }
            if !word.starts_with('-')
                && let Some(at) = given.values.iter().position(Option::is_none)
            {
                given.fill(at, word.clone())?;
                continue;
            }
            given.extra.push(word.clone());
        }
        Ok(given)
    }

    /// Give the argument declared at `at` a value.
    ///
    /// # Errors
    ///
    /// When it already has one, or the value is not one of its choices.
    pub fn fill(&mut self, at: usize, value: String) -> Result<(), ArgumentError> {
        let argument = &self.declared[at];
        if let Some(first) = &self.values[at] {
            return Err(ArgumentError::Twice {
                name: argument.name.to_string(),
                first: first.clone(),
                second: value,
            });
        }
        if !argument.choices.is_empty() && !argument.choices.iter().any(|choice| *choice == value) {
            return Err(ArgumentError::NotAChoice {
                name: argument.name.to_string(),
                value,
                choices: argument.choices.iter().map(ToString::to_string).collect(),
            });
        }
        self.values[at] = Some(value);
        Ok(())
    }

    /// The arguments that are required, not given, and have no default — the
    /// ones there is no answer for without asking — by declaration index.
    pub fn missing(&self) -> impl Iterator<Item = (usize, &'d TaskArgument)> + '_ {
        self.declared.iter().enumerate().filter(|(at, argument)| {
            self.values[*at].is_none() && argument.is_required() && argument.default.is_none()
        })
    }

    /// Every argument with the value it ends up with, defaults applied.
    ///
    /// # Errors
    ///
    /// [`ArgumentError::Missing`], naming every argument [`Self::missing`]
    /// still reports.
    pub fn resolve(self) -> Result<Resolved, ArgumentError> {
        let missing: Vec<Missing> = self
            .missing()
            .map(|(_, argument)| Missing {
                name: argument.name.to_string(),
                description: argument.description.as_ref().map(ToString::to_string),
                choices: argument.choices.iter().map(ToString::to_string).collect(),
            })
            .collect();
        if !missing.is_empty() {
            return Err(ArgumentError::Missing(missing));
        }
        let values = self
            .declared
            .iter()
            .zip(self.values)
            .filter_map(|(argument, value)| {
                let value = value.or_else(|| argument.default.as_ref().map(ToString::to_string))?;
                Some((argument.name.to_string(), value))
            })
            .collect();
        Ok(Resolved {
            values,
            extra: self.extra,
        })
    }
}

/// What a task is run with: its declared arguments' values, and the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resolved {
    /// Each declared argument that has a value, by name, in declaration order.
    pub values: Vec<(String, String)>,
    /// Every word no argument declares, as written.
    pub extra: Vec<String>,
}

impl Resolved {
    /// Words that declare nothing: what `uf run` did before `args` existed.
    #[must_use]
    pub fn undeclared(words: &[String]) -> Self {
        Self {
            values: Vec::new(),
            extra: words.to_vec(),
        }
    }

    /// Whether there is nothing to append.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.extra.is_empty()
    }

    /// What is appended to the command: each value quoted as one word, then
    /// the undeclared words as they were written.
    #[must_use]
    pub fn command_text(&self) -> String {
        let mut text = String::new();
        let words = self
            .values
            .iter()
            .map(|(_, value)| quote(value))
            .chain(self.extra.iter().map(|word| Cow::Borrowed(word.as_str())));
        for word in words {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&word);
        }
        text
    }

    /// The same, as separate words, for a runner that takes an argument
    /// vector rather than a command line.
    #[must_use]
    pub fn words(&self) -> Vec<String> {
        self.values
            .iter()
            .map(|(_, value)| value.clone())
            .chain(self.extra.iter().cloned())
            .collect()
    }
}

/// `word` as a POSIX shell reads one word: unchanged when nothing in it is
/// special, and single-quoted otherwise.
#[must_use]
pub fn quote(word: &str) -> Cow<'_, str> {
    let plain = !word.is_empty()
        && word
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./:=@%+,".contains(character));
    if plain {
        return Cow::Borrowed(word);
    }
    Cow::Owned(format!("'{}'", word.replace('\'', r"'\''")))
}

#[cfg(test)]
mod tests {
    use compact_str::CompactString;

    use super::*;

    fn argument(name: &str) -> TaskArgument {
        TaskArgument::named(name)
    }

    fn choices(name: &str, values: &[&str]) -> TaskArgument {
        let mut argument = argument(name);
        argument.choices = values
            .iter()
            .map(|value| CompactString::new(value))
            .collect();
        argument
    }

    fn defaulted(mut argument: TaskArgument, value: &str) -> TaskArgument {
        argument.default = Some(value.into());
        argument
    }

    fn optional(mut argument: TaskArgument) -> TaskArgument {
        argument.required = Some(false);
        argument
    }

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(ToString::to_string).collect()
    }

    fn resolve(declared: &[TaskArgument], line: &[&str]) -> Result<Resolved, ArgumentError> {
        Given::parse(declared, &words(line))?.resolve()
    }

    fn pairs(resolved: &Resolved) -> Vec<(&str, &str)> {
        resolved
            .values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect()
    }

    #[test]
    fn positional_words_fill_the_arguments_in_declaration_order() {
        let declared = [
            choices("target", &["staging", "production"]),
            argument("tag"),
        ];
        let resolved = resolve(&declared, &["production", "v1"]).unwrap();
        assert_eq!(pairs(&resolved), [("target", "production"), ("tag", "v1")]);
        assert_eq!(resolved.command_text(), "production v1");
    }

    #[test]
    fn a_named_argument_is_placed_where_it_is_declared() {
        let declared = [
            choices("target", &["staging", "production"]),
            argument("tag"),
        ];
        let resolved = resolve(&declared, &["--tag", "v1", "--target=staging"]).unwrap();
        assert_eq!(resolved.command_text(), "staging v1");

        // Mixed: the named one is taken out first, and the positional word
        // fills what is left.
        let resolved = resolve(&declared, &["--target", "staging", "v2"]).unwrap();
        assert_eq!(pairs(&resolved), [("target", "staging"), ("tag", "v2")]);
    }

    #[test]
    fn words_nobody_declared_follow_the_declared_values_untouched() {
        let declared = [argument("target")];
        let resolved = resolve(&declared, &["--dry-run", "staging", "more", "*.js"]).unwrap();
        assert_eq!(pairs(&resolved), [("target", "staging")]);
        assert_eq!(resolved.extra, ["--dry-run", "more", "*.js"]);
        assert_eq!(resolved.command_text(), "staging --dry-run more *.js");
    }

    #[test]
    fn a_task_that_declares_nothing_appends_what_it_always_did() {
        let resolved = resolve(&[], &["--loud", "a b"]).unwrap();
        assert_eq!(resolved, Resolved::undeclared(&words(&["--loud", "a b"])));
        assert_eq!(resolved.command_text(), "--loud a b");
    }

    #[test]
    fn a_value_outside_the_choices_is_refused_by_name() {
        let declared = [choices("target", &["staging", "production"])];
        let error = resolve(&declared, &["prod"]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "<target> cannot be \"prod\"; it is one of: staging, production"
        );
        let error = resolve(&declared, &["--target", "qa"]).unwrap_err();
        assert!(
            matches!(error, ArgumentError::NotAChoice { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_missing_required_argument_is_named_with_its_choices() {
        let mut target = choices("target", &["staging", "production"]);
        target.description = Some("Where to deploy".into());
        let declared = [target, argument("tag")];
        let error = resolve(&declared, &[]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "missing <target>, <tag>\n  <target>  Where to deploy — one of: staging, production\n  <tag>"
        );
    }

    #[test]
    fn a_default_fills_what_was_not_given_and_is_never_missing() {
        let declared = [
            defaulted(choices("region", &["us", "eu"]), "eu"),
            argument("tag"),
        ];
        let given = Given::parse(&declared, &words(&["--tag", "v1"])).unwrap();
        assert_eq!(given.missing().count(), 0);
        assert_eq!(given.resolve().unwrap().command_text(), "eu v1");
    }

    #[test]
    fn an_optional_argument_with_no_default_is_left_out() {
        let declared = [argument("target"), optional(argument("note"))];
        let resolved = resolve(&declared, &["staging"]).unwrap();
        assert_eq!(pairs(&resolved), [("target", "staging")]);
    }

    #[test]
    fn filling_what_is_missing_is_validated_like_the_command_line() {
        let declared = [choices("target", &["staging", "production"])];
        let mut given = Given::parse(&declared, &[]).unwrap();
        let (at, _) = given.missing().next().unwrap();
        assert!(given.fill(at, "nope".into()).is_err());
        given.fill(at, "staging".into()).unwrap();
        assert_eq!(given.missing().count(), 0);
        assert_eq!(given.resolve().unwrap().command_text(), "staging");
    }

    #[test]
    fn an_argument_given_twice_or_without_a_value_is_an_error() {
        let declared = [argument("target")];
        assert!(matches!(
            resolve(&declared, &["a", "--target", "b"]),
            Err(ArgumentError::Twice { .. })
        ));
        assert_eq!(
            resolve(&declared, &["--target"]).unwrap_err(),
            ArgumentError::NoValue("target".into())
        );
    }

    #[test]
    fn a_value_is_one_word_whatever_it_contains() {
        let declared = [argument("message")];
        let resolved = resolve(&declared, &["--message", "it's done"]).unwrap();
        assert_eq!(resolved.command_text(), r"'it'\''s done'");
        assert_eq!(resolved.words(), ["it's done"]);
        assert_eq!(quote(""), "''");
        assert_eq!(quote("a/b.js"), "a/b.js");
        assert_eq!(quote("$HOME"), "'$HOME'");
    }

    #[test]
    fn a_declaration_uf_run_cannot_fill_is_refused() {
        assert_eq!(check(&[argument("target"), argument("tag")]), Ok(()));
        assert_eq!(
            check(&[argument("--target")]),
            Err(DeclarationError::BadName("--target".into()))
        );
        assert_eq!(
            check(&[argument("a"), argument("a")]),
            Err(DeclarationError::Duplicate("a".into()))
        );
        assert!(matches!(
            check(&[defaulted(choices("a", &["x"]), "y")]),
            Err(DeclarationError::DefaultNotAChoice { .. })
        ));
        let mut both = defaulted(argument("a"), "x");
        both.required = Some(true);
        assert!(matches!(
            check(&[both]),
            Err(DeclarationError::RequiredWithDefault(_))
        ));
        assert!(matches!(
            check(&[optional(argument("a")), argument("b")]),
            Err(DeclarationError::OptionalBeforeRequired { .. })
        ));
        // After a default, anything goes: the default keeps its place.
        assert_eq!(
            check(&[defaulted(argument("a"), "x"), argument("b")]),
            Ok(())
        );
        assert_eq!(
            check(&[
                argument("a"),
                optional(argument("b")),
                optional(argument("c"))
            ]),
            Ok(())
        );
    }
}
