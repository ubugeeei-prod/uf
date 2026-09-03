//! Lowering Flow-only syntax to JavaScript, by Flow's own rules.
//!
//! Each pass here is a port of the corresponding transform in Meta's
//! `hermes-parser` package — the code Flow's toolchain runs when it emits
//! Babel-compatible output — kept structurally close to the original so the
//! two can be compared line by line:
//!
//! | pass           | upstream source                     |
//! | -------------- | ----------------------------------- |
//! | [`components`] | `estree/TransformComponentSyntax.js` |
//! | [`matches`]    | `estree/TransformMatchSyntax.js`     |
//! | [`enums`]      | `estree/TransformEnumSyntax.js`      |
//! | [`strip`]      | `estree/StripFlowTypes.js`           |
//!
//! All four work on the ESTree `serde_json::Value` tree from [`crate::estree`]
//! and run in that order: components and hooks become functions, `match`
//! becomes conditions, enums become runtime objects, and finally every type is
//! erased. Running the strip last is what lets the earlier passes read the
//! annotations they need (a component's prop types, say) before they are gone.

pub mod builders;
pub mod components;
pub mod enums;
pub mod matches;
pub mod strip;

use serde_json::Value;

use crate::TransformError;

/// Deepest tree the walker follows before refusing the module.
///
/// The parser has already bounded the input; this is the belt to that
/// suspenders, so an adversarially nested program cannot overflow the stack
/// in a rewrite pass.
pub const MAX_DEPTH: usize = 1024;

/// What a pass found out about the module, for the stages after it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lowered {
    /// The module declares at least one `component` or `hook`, so the React
    /// Compiler has something to do in `syntax` mode.
    pub may_compile: bool,
}

/// Run every lowering pass over `program`, in order.
///
/// # Errors
///
/// [`TransformError::Lowering`] when a construct is refused (the same cases
/// `hermes-parser` raises a syntax error for), and
/// [`TransformError::Internal`] when the tree is not the shape the parser
/// promised.
pub fn lower(program: &mut Value, source: &str) -> Result<Lowered, TransformError> {
    let may_compile = components::lower(program)?;
    matches::lower(program)?;
    enums::lower(program, source)?;
    strip::lower(program)?;
    Ok(Lowered { may_compile })
}

/// What a visitor asks to happen to the node it was handed.
#[derive(Debug)]
pub enum Edit {
    /// Leave the node in place.
    Keep,
    /// Put this node in its place.
    Replace(Value),
    /// Drop the node. In a list it disappears; in a single slot it becomes `null`.
    Remove,
    /// Put these nodes in its place. Only valid for a node in a list.
    Splice(Vec<Value>),
}

/// The keys the walker never descends into.
///
/// `loc`/`range` are positions, `comments` on a program is the comment list
/// and the `*Comments` fields are the comments the parser attached to a node,
/// and `type` is the discriminator. Nothing else on a node is anything but a
/// child node, a list of them, or a scalar.
const SKIPPED_KEYS: [&str; 7] = [
    "type",
    "loc",
    "range",
    "comments",
    "leadingComments",
    "trailingComments",
    "innerComments",
];

/// The node type of `value`, when it is an AST node.
#[must_use]
pub fn node_type(value: &Value) -> Option<&str> {
    value.as_object()?.get("type")?.as_str()
}

/// Whether `value` is an AST node: an object with a string `type`.
#[must_use]
pub fn is_node(value: &Value) -> bool {
    node_type(value).is_some()
}

/// Walk `node` post-order, offering every descendant and then the node itself
/// to `visit`, and applying what it asks.
///
/// Children are rewritten before their parent, so a visitor that replaces a
/// node sees already-lowered children in it and never has to recurse. Lists
/// honour [`Edit::Remove`] and [`Edit::Splice`]; a single child slot turns a
/// removal into `null`.
///
/// # Errors
///
/// [`TransformError::Internal`] past [`MAX_DEPTH`] or when a splice is asked
/// of a single slot.
pub fn transform_post(
    node: &mut Value,
    visit: &mut dyn FnMut(&mut Value) -> Result<Edit, TransformError>,
) -> Result<Edit, TransformError> {
    transform_post_at(node, visit, 0)
}

fn transform_post_at(
    node: &mut Value,
    visit: &mut dyn FnMut(&mut Value) -> Result<Edit, TransformError>,
    depth: usize,
) -> Result<Edit, TransformError> {
    if depth > MAX_DEPTH {
        return Err(TransformError::Internal(format!(
            "syntax tree deeper than {MAX_DEPTH} levels"
        )));
    }
    let Some(object) = node.as_object_mut() else {
        return Ok(Edit::Keep);
    };
    if !object.get("type").is_some_and(Value::is_string) {
        return Ok(Edit::Keep);
    }

    let keys: Vec<String> = object
        .keys()
        .filter(|key| !SKIPPED_KEYS.contains(&key.as_str()))
        .cloned()
        .collect();
    for key in keys {
        let Some(child) = object.get_mut(&key) else {
            continue;
        };
        match child {
            Value::Array(items) => {
                let mut rebuilt = Vec::with_capacity(items.len());
                for mut item in std::mem::take(items) {
                    if !is_node(&item) {
                        rebuilt.push(item);
                        continue;
                    }
                    match transform_post_at(&mut item, visit, depth + 1)? {
                        Edit::Keep => rebuilt.push(item),
                        Edit::Replace(next) => rebuilt.push(next),
                        Edit::Remove => {}
                        Edit::Splice(nodes) => rebuilt.extend(nodes),
                    }
                }
                *items = rebuilt;
            }
            child if is_node(child) => match transform_post_at(child, visit, depth + 1)? {
                Edit::Keep => {}
                Edit::Replace(next) => *child = next,
                Edit::Remove => *child = Value::Null,
                Edit::Splice(_) => {
                    return Err(TransformError::Internal(format!(
                        "cannot splice several nodes into the single slot `{key}`"
                    )));
                }
            },
            _ => {}
        }
    }

    visit(node)
}

/// Visit every node pre-order, read-only.
pub fn walk(node: &Value, visit: &mut dyn FnMut(&Value)) {
    walk_at(node, visit, 0);
}

fn walk_at(node: &Value, visit: &mut dyn FnMut(&Value), depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    let Some(object) = node.as_object() else {
        return;
    };
    if !object.get("type").is_some_and(Value::is_string) {
        return;
    }
    visit(node);
    for (key, child) in object {
        if SKIPPED_KEYS.contains(&key.as_str()) {
            continue;
        }
        match child {
            Value::Array(items) => {
                for item in items {
                    walk_at(item, visit, depth + 1);
                }
            }
            child => walk_at(child, visit, depth + 1),
        }
    }
}

/// A lowering error at a node's position.
pub(crate) fn refuse(node: &Value, message: impl Into<String>) -> TransformError {
    let start = node.get("loc").and_then(|loc| loc.get("start"));
    let number = |key: &str| {
        start
            .and_then(|position| position.get(key))
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    };
    TransformError::Lowering {
        message: message.into(),
        line: number("line"),
        column: number("column"),
    }
}

/// Copy the `loc` and `range` of `from` onto a freshly built node.
pub(crate) fn with_position_of(mut node: Value, from: &Value) -> Value {
    if let (Some(object), Some(source)) = (node.as_object_mut(), from.as_object()) {
        for key in ["loc", "range"] {
            if let Some(value) = source.get(key) {
                object.insert(key.to_owned(), value.clone());
            }
        }
    }
    node
}

/// Take a field out of a node, leaving `null` behind.
pub(crate) fn take(node: &mut Value, key: &str) -> Value {
    node.as_object_mut()
        .and_then(|object| object.get_mut(key))
        .map(Value::take)
        .unwrap_or(Value::Null)
}

/// A node's field as a string.
pub(crate) fn str_field<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.get(key).and_then(Value::as_str)
}

/// A node's field as a bool, `false` when absent.
pub(crate) fn bool_field(node: &Value, key: &str) -> bool {
    node.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// A node's field as a list, empty when absent.
pub(crate) fn list_field<'a>(node: &'a Value, key: &str) -> &'a [Value] {
    node.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn post_order_visits_children_before_parents() {
        let mut tree = json!({
            "type": "Program",
            "body": [{ "type": "A", "child": { "type": "B" } }, { "type": "C" }],
        });
        let mut order = Vec::new();
        transform_post(&mut tree, &mut |node| {
            order.push(node_type(node).unwrap().to_owned());
            Ok(Edit::Keep)
        })
        .unwrap();
        assert_eq!(order, ["B", "A", "C", "Program"]);
    }

    #[test]
    fn lists_honour_removal_and_splicing() {
        let mut tree = json!({
            "type": "Program",
            "body": [{ "type": "Drop" }, { "type": "Twice" }, { "type": "Keep" }],
        });
        transform_post(&mut tree, &mut |node| {
            Ok(match node_type(node) {
                Some("Drop") => Edit::Remove,
                Some("Twice") => Edit::Splice(vec![json!({"type": "X"}), json!({"type": "Y"})]),
                _ => Edit::Keep,
            })
        })
        .unwrap();
        let types: Vec<&str> = tree["body"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node_type(node).unwrap())
            .collect();
        assert_eq!(types, ["X", "Y", "Keep"]);
    }

    #[test]
    fn a_single_slot_cannot_take_a_splice() {
        let mut tree = json!({ "type": "A", "child": { "type": "B" } });
        let error = transform_post(&mut tree, &mut |node| {
            Ok(if node_type(node) == Some("B") {
                Edit::Splice(vec![])
            } else {
                Edit::Keep
            })
        })
        .unwrap_err();
        assert!(matches!(error, TransformError::Internal(_)));
    }

    #[test]
    fn refuses_a_tree_deeper_than_the_ceiling() {
        // Building and dropping a tree this deep is itself recursive, so the
        // test runs on a thread with room for it.
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(|| {
                let mut tree = json!({ "type": "Leaf" });
                for _ in 0..=MAX_DEPTH {
                    tree = json!({ "type": "Wrap", "inner": tree });
                }
                let error = transform_post(&mut tree, &mut |_| Ok(Edit::Keep)).unwrap_err();
                assert!(matches!(error, TransformError::Internal(_)));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
