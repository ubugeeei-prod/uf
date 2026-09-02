//! Reading one `stylex.create` or `stylex.defineVars` argument.
//!
//! The walk is a work queue rather than a recursion, and its element type is
//! [`Level`] — a closed set of three. That is what bounds the descent: there is
//! no fourth level to push, so an object nested any deeper is a typed error
//! rather than another native stack frame.

use std::collections::VecDeque;

use compact_str::CompactString;

use crate::class::variable_name;
use crate::condition::StyleCondition;
use crate::error::{MAX_DECLARATIONS, MAX_OBJECT_DEPTH, StyleXError};
use crate::property::{css_property_name, is_forbidden_key, is_valid_key};

use super::bindings::ModuleBindings;
use super::object::{self, Cursor, Entry};
use super::{Declaration, Namespace, Variable};

/// Which keys the object at one queue entry holds.
#[derive(Debug, Clone)]
enum Level {
    /// Inside one namespace: keys are properties or conditions.
    Namespace { namespace: usize },
    /// Inside one property: keys are conditions.
    Conditions {
        namespace: usize,
        key: CompactString,
        property: CompactString,
    },
    /// Inside one condition: keys are properties.
    Properties {
        namespace: usize,
        condition: StyleCondition,
    },
}

/// One object still to be walked.
#[derive(Debug, Clone)]
struct Pending {
    open: usize,
    level: Level,
}

/// Read the namespaces out of the object handed to `stylex.create`.
pub fn create_namespaces(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    open: usize,
) -> Result<Vec<Namespace>, StyleXError> {
    let mut namespaces: Vec<Namespace> = Vec::new();
    let mut queue: VecDeque<Pending> = VecDeque::new();
    let mut declarations = 0usize;

    for entry in object::entries(cursor, open)? {
        check_key(&entry)?;
        let Some(child) = object::object_at(cursor, entry.value_start, entry.value_end) else {
            return Err(StyleXError::ExpectedObjectLiteral { at: entry.at });
        };
        namespaces.push(Namespace {
            name: entry.key,
            declarations: Vec::new(),
        });
        queue.push_back(Pending {
            open: child,
            level: Level::Namespace {
                namespace: namespaces.len() - 1,
            },
        });
    }

    while let Some(pending) = queue.pop_front() {
        for entry in object::entries(cursor, pending.open)? {
            declarations += 1;
            if declarations > MAX_DECLARATIONS {
                return Err(StyleXError::TooManyDeclarations {
                    limit: MAX_DECLARATIONS,
                });
            }
            step(
                cursor,
                bindings,
                &pending.level,
                entry,
                &mut namespaces,
                &mut queue,
            )?;
        }
    }

    for namespace in &mut namespaces {
        namespace.declarations.sort_by_key(|entry| entry.at.offset);
    }
    Ok(namespaces)
}

/// Handle one `key: value` pair at one level of a `stylex.create` object.
fn step(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    level: &Level,
    entry: Entry,
    namespaces: &mut [Namespace],
    queue: &mut VecDeque<Pending>,
) -> Result<(), StyleXError> {
    let nested = object::object_at(cursor, entry.value_start, entry.value_end);
    match level {
        Level::Namespace { namespace } => {
            if StyleCondition::is_condition_key(&entry.key) {
                let condition = condition_of(&entry)?;
                let Some(child) = nested else {
                    return Err(StyleXError::ExpectedObjectLiteral { at: entry.at });
                };
                queue.push_back(Pending {
                    open: child,
                    level: Level::Properties {
                        namespace: *namespace,
                        condition,
                    },
                });
                return Ok(());
            }
            check_key(&entry)?;
            let property = css_property_name(&entry.key);
            match nested {
                Some(child) => queue.push_back(Pending {
                    open: child,
                    level: Level::Conditions {
                        namespace: *namespace,
                        key: entry.key,
                        property,
                    },
                }),
                None => push_declaration(
                    cursor,
                    bindings,
                    namespaces,
                    *namespace,
                    entry,
                    property,
                    StyleCondition::Base,
                )?,
            }
        }
        Level::Conditions {
            namespace,
            key,
            property,
        } => {
            reject_nesting(&entry, nested)?;
            let condition = condition_of(&entry)?;
            let key = key.clone();
            let property = property.clone();
            push_declaration_with_key(
                cursor, bindings, namespaces, *namespace, entry, key, property, condition,
            )?;
        }
        Level::Properties {
            namespace,
            condition,
        } => {
            reject_nesting(&entry, nested)?;
            check_key(&entry)?;
            let property = css_property_name(&entry.key);
            push_declaration(
                cursor,
                bindings,
                namespaces,
                *namespace,
                entry,
                property,
                condition.clone(),
            )?;
        }
    }
    Ok(())
}

/// Refuse an object where only a value can go.
fn reject_nesting(entry: &Entry, nested: Option<usize>) -> Result<(), StyleXError> {
    match nested {
        Some(_) => Err(StyleXError::NestingTooDeep {
            at: entry.at,
            limit: MAX_OBJECT_DEPTH,
        }),
        None => Ok(()),
    }
}

/// Read a condition key, or say the key is not one.
fn condition_of(entry: &Entry) -> Result<StyleCondition, StyleXError> {
    StyleCondition::parse(&entry.key).ok_or_else(|| StyleXError::InvalidKey {
        at: entry.at,
        key: entry.key.clone(),
    })
}

/// Check a namespace or property key before it reaches generated code.
fn check_key(entry: &Entry) -> Result<(), StyleXError> {
    if is_forbidden_key(&entry.key) {
        return Err(StyleXError::ForbiddenKey {
            at: entry.at,
            key: entry.key.clone(),
        });
    }
    if !is_valid_key(&entry.key) {
        return Err(StyleXError::InvalidKey {
            at: entry.at,
            key: entry.key.clone(),
        });
    }
    Ok(())
}

/// Resolve a value and attach it to a namespace under its own key.
fn push_declaration(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    namespaces: &mut [Namespace],
    namespace: usize,
    entry: Entry,
    property: CompactString,
    condition: StyleCondition,
) -> Result<(), StyleXError> {
    let key = entry.key.clone();
    push_declaration_with_key(
        cursor, bindings, namespaces, namespace, entry, key, property, condition,
    )
}

/// Resolve a value and attach it to a namespace under `key`.
#[allow(clippy::too_many_arguments)]
fn push_declaration_with_key(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    namespaces: &mut [Namespace],
    namespace: usize,
    entry: Entry,
    key: CompactString,
    property: CompactString,
    condition: StyleCondition,
) -> Result<(), StyleXError> {
    let value = object::value(cursor, bindings, entry.value_start, entry.value_end)?;
    if let Some(target) = namespaces.get_mut(namespace) {
        target.declarations.push(Declaration {
            key,
            property,
            condition,
            value,
            at: entry.at,
        });
    }
    Ok(())
}

/// Read the entries out of the object handed to `stylex.defineVars`.
pub fn variables(
    cursor: Cursor<'_>,
    bindings: &ModuleBindings,
    binding: &str,
    open: usize,
) -> Result<Vec<Variable>, StyleXError> {
    let mut found = Vec::new();
    for entry in object::entries(cursor, open)? {
        check_key(&entry)?;
        if found.len() >= MAX_DECLARATIONS {
            return Err(StyleXError::TooManyDeclarations {
                limit: MAX_DECLARATIONS,
            });
        }
        let value = object::value(cursor, bindings, entry.value_start, entry.value_end)?;
        found.push(Variable {
            name: variable_name(binding, &entry.key),
            key: entry.key,
            value,
            at: entry.at,
        });
    }
    Ok(found)
}
