//! `match` expressions and statements become conditions and bindings.
//!
//! A port of `hermes-parser`'s `TransformMatchSyntax.js`. A pattern is
//! analysed into *conditions* (what must be true of the value) and *bindings*
//! (what names it introduces), and each case becomes a test over those
//! conditions with its bindings declared inside. The generated tests are the
//! ones Flow documents — `typeof x === "object" && x !== null`, `"k" in x`,
//! `Array.isArray(x) && x.length === n` — so an author can read the output.
//!
//! An expression with no bindings over a side-effect-free argument becomes a
//! nested conditional; everything else becomes an immediately invoked arrow
//! function, and a statement becomes a labelled block that each case
//! `break`s out of. Falling off the end throws, unless a wildcard catches
//! everything.

use serde_json::{Value, json};
use uf_infra::FxHashSet;

use super::builders::{
    binary, block, call, conditional, conjunction, disjunction, ident, if_statement, iife, member,
    null_literal, number_literal, property, rest_element, return_statement, string_literal,
    throw_statement, typeof_is, unary, variable_declaration,
};
use super::{Edit, list_field, node_type, refuse, str_field, take, transform_post, walk};
use crate::TransformError;

/// Lower every `match` in `program`.
pub fn lower(program: &mut Value) -> Result<(), TransformError> {
    let mut names = GenId::new(program);
    transform_post(program, &mut |node| {
        Ok(match node_type(node) {
            Some("MatchExpression") => Edit::Replace(super::with_position_of(
                map_match_expression(node, &mut names)?,
                node,
            )),
            Some("MatchStatement") => Edit::Replace(super::with_position_of(
                map_match_statement(node, &mut names)?,
                node,
            )),
            _ => Edit::Keep,
        })
    })?;
    Ok(())
}

/// Generated identifiers that cannot collide with a name the module uses.
///
/// `hermes-parser` records every identifier in the program before it
/// generates any; so does this.
struct GenId {
    used: FxHashSet<String>,
    next: usize,
}

impl GenId {
    fn new(program: &Value) -> Self {
        let mut used = FxHashSet::default();
        walk(program, &mut |node| {
            if node_type(node) == Some("Identifier")
                && let Some(name) = str_field(node, "name")
            {
                used.insert(name.to_owned());
            }
        });
        Self { used, next: 0 }
    }

    fn id(&mut self) -> Value {
        loop {
            let candidate = format!("$$gen$m{}", self.next);
            self.next += 1;
            if !self.used.contains(&candidate) {
                self.used.insert(candidate.clone());
                return ident(&candidate);
            }
        }
    }
}

/// A path from the matched value to a position inside it.
type Key = Vec<Value>;

/// What must hold at one position for a pattern to match.
enum Condition {
    Eq {
        key: Key,
        arg: Value,
    },
    IsNan {
        key: Key,
    },
    Array {
        key: Key,
        length: usize,
        at_least: bool,
    },
    Object {
        key: Key,
    },
    InstanceOf {
        key: Key,
        constructor: Value,
    },
    PropExists {
        key: Key,
        name: String,
    },
    Or {
        alternatives: Vec<Vec<Condition>>,
    },
}

/// A name a pattern introduces.
enum Binding {
    Id {
        key: Key,
        kind: String,
        id: Value,
    },
    ArrayRest {
        key: Key,
        kind: String,
        id: Value,
        exclude: usize,
    },
    ObjectRest {
        key: Key,
        kind: String,
        id: Value,
        exclude: Vec<Value>,
    },
}

struct Analysis {
    conditions: Vec<Condition>,
    bindings: Vec<Binding>,
}

fn object_key_name(node: &Value) -> String {
    match node_type(node) {
        Some("Identifier") => str_field(node, "name").unwrap_or("").to_owned(),
        _ => match &node["value"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => str_field(node, "raw").unwrap_or("").to_owned(),
        },
    }
}

fn convert_member_pattern(pattern: &Value) -> Value {
    let base = &pattern["base"];
    let object = if node_type(base) == Some("MatchIdentifierPattern") {
        base["id"].clone()
    } else {
        convert_member_pattern(base)
    };
    let property = pattern["property"].clone();
    let computed = node_type(&property) != Some("Identifier");
    super::with_position_of(member(object, property, computed), pattern)
}

fn check_duplicate(
    seen: &mut FxHashSet<String>,
    node: &Value,
    name: &str,
) -> Result<(), TransformError> {
    if !seen.insert(name.to_owned()) {
        return Err(refuse(
            node,
            format!("Duplicate variable name '{name}' in match case pattern."),
        ));
    }
    Ok(())
}

fn check_kind(node: &Value, kind: &str) -> Result<(), TransformError> {
    if kind == "var" {
        return Err(refuse(
            node,
            "'var' bindings are not allowed. Use 'const' or 'let'.",
        ));
    }
    Ok(())
}

/// Whether a property pattern needs an explicit `"k" in x` test.
///
/// A literal test implies the property exists; a test that could be an
/// equality with `undefined` does not.
fn needs_prop_exists(pattern: &Value) -> bool {
    match node_type(pattern) {
        Some(
            "MatchWildcardPattern"
            | "MatchBindingPattern"
            | "MatchIdentifierPattern"
            | "MatchMemberPattern",
        ) => true,
        Some("MatchAsPattern") => needs_prop_exists(&pattern["pattern"]),
        Some("MatchOrPattern") => list_field(pattern, "patterns")
            .iter()
            .any(needs_prop_exists),
        _ => false,
    }
}

fn analyze_properties(
    key: &Key,
    pattern: &Value,
    seen: &mut FxHashSet<String>,
    properties: &[Value],
    rest: &Value,
) -> Result<Analysis, TransformError> {
    let mut conditions = Vec::new();
    let mut bindings = Vec::new();
    let mut object_keys = Vec::new();
    let mut names = FxHashSet::default();

    for prop in properties {
        let object_key = prop["key"].clone();
        let name = object_key_name(&object_key);
        if !names.insert(name.clone()) {
            return Err(refuse(
                &prop["pattern"],
                format!("Duplicate property name '{name}' in match object pattern."),
            ));
        }
        object_keys.push(object_key.clone());
        let mut property_key = key.clone();
        property_key.push(object_key);
        if needs_prop_exists(&prop["pattern"]) {
            conditions.push(Condition::PropExists {
                key: key.clone(),
                name,
            });
        }
        let child = analyze_pattern(&prop["pattern"], property_key, seen)?;
        conditions.extend(child.conditions);
        bindings.extend(child.bindings);
    }

    if node_type(rest) == Some("MatchRestPattern")
        && node_type(&rest["argument"]) == Some("MatchBindingPattern")
    {
        let argument = &rest["argument"];
        let id = argument["id"].clone();
        let kind = str_field(argument, "kind").unwrap_or("const").to_owned();
        check_duplicate(seen, argument, str_field(&id, "name").unwrap_or(""))?;
        check_kind(pattern, &kind)?;
        bindings.push(Binding::ObjectRest {
            key: key.clone(),
            kind,
            id,
            exclude: object_keys,
        });
    }

    Ok(Analysis {
        conditions,
        bindings,
    })
}

fn constructor_expression(constructor: &Value) -> Value {
    match node_type(constructor) {
        Some("MatchIdentifierPattern") => constructor["id"].clone(),
        _ => convert_member_pattern(constructor),
    }
}

fn analyze_pattern(
    pattern: &Value,
    key: Key,
    seen: &mut FxHashSet<String>,
) -> Result<Analysis, TransformError> {
    let none = Analysis {
        conditions: Vec::new(),
        bindings: Vec::new(),
    };
    match node_type(pattern) {
        Some("MatchWildcardPattern") => Ok(none),
        Some("MatchLiteralPattern") => Ok(Analysis {
            conditions: vec![Condition::Eq {
                key,
                arg: pattern["literal"].clone(),
            }],
            bindings: Vec::new(),
        }),
        Some("MatchUnaryPattern") => {
            let argument = &pattern["argument"];
            if argument["value"].as_f64() == Some(0.0) {
                return Err(refuse(
                    pattern,
                    "'+0' and '-0' are not yet supported in match unary patterns.",
                ));
            }
            let operator = str_field(pattern, "operator").unwrap_or("-");
            let arg = super::with_position_of(unary(operator, argument.clone()), pattern);
            Ok(Analysis {
                conditions: vec![Condition::Eq { key, arg }],
                bindings: Vec::new(),
            })
        }
        Some("MatchIdentifierPattern") => {
            let id = pattern["id"].clone();
            let condition = if str_field(&id, "name") == Some("NaN") {
                Condition::IsNan { key }
            } else {
                Condition::Eq { key, arg: id }
            };
            Ok(Analysis {
                conditions: vec![condition],
                bindings: Vec::new(),
            })
        }
        Some("MatchMemberPattern") => Ok(Analysis {
            conditions: vec![Condition::Eq {
                key,
                arg: convert_member_pattern(pattern),
            }],
            bindings: Vec::new(),
        }),
        Some("MatchBindingPattern") => {
            let id = pattern["id"].clone();
            let kind = str_field(pattern, "kind").unwrap_or("const").to_owned();
            check_duplicate(seen, pattern, str_field(&id, "name").unwrap_or(""))?;
            check_kind(pattern, &kind)?;
            Ok(Analysis {
                conditions: Vec::new(),
                bindings: vec![Binding::Id { key, kind, id }],
            })
        }
        Some("MatchAsPattern") => {
            let inner = &pattern["pattern"];
            if node_type(inner) == Some("MatchBindingPattern") {
                return Err(refuse(
                    pattern,
                    "Match 'as' patterns are not allowed directly on binding patterns.",
                ));
            }
            let mut analysis = analyze_pattern(inner, key.clone(), seen)?;
            let target = &pattern["target"];
            let (id, kind) = if node_type(target) == Some("MatchBindingPattern") {
                (
                    target["id"].clone(),
                    str_field(target, "kind").unwrap_or("const").to_owned(),
                )
            } else {
                (target.clone(), String::from("const"))
            };
            check_duplicate(seen, pattern, str_field(&id, "name").unwrap_or(""))?;
            check_kind(pattern, &kind)?;
            analysis.bindings.push(Binding::Id { key, kind, id });
            Ok(analysis)
        }
        Some("MatchArrayPattern") => {
            let elements = list_field(pattern, "elements");
            let rest = &pattern["rest"];
            let has_rest = node_type(rest) == Some("MatchRestPattern");
            let mut conditions = vec![Condition::Array {
                key: key.clone(),
                length: elements.len(),
                at_least: has_rest,
            }];
            let mut bindings = Vec::new();
            for (index, element) in elements.iter().enumerate() {
                let mut element_key = key.clone();
                element_key.push(number_literal(index));
                let child = analyze_pattern(element, element_key, seen)?;
                conditions.extend(child.conditions);
                bindings.extend(child.bindings);
            }
            if has_rest && node_type(&rest["argument"]) == Some("MatchBindingPattern") {
                let argument = &rest["argument"];
                let id = argument["id"].clone();
                let kind = str_field(argument, "kind").unwrap_or("const").to_owned();
                check_duplicate(seen, argument, str_field(&id, "name").unwrap_or(""))?;
                check_kind(pattern, &kind)?;
                bindings.push(Binding::ArrayRest {
                    key,
                    kind,
                    id,
                    exclude: elements.len(),
                });
            }
            Ok(Analysis {
                conditions,
                bindings,
            })
        }
        Some("MatchObjectPattern") => {
            let inner = analyze_properties(
                &key,
                pattern,
                seen,
                list_field(pattern, "properties"),
                &pattern["rest"],
            )?;
            let mut conditions = vec![Condition::Object { key }];
            conditions.extend(inner.conditions);
            Ok(Analysis {
                conditions,
                bindings: inner.bindings,
            })
        }
        Some("MatchInstancePattern") => {
            let properties = &pattern["properties"];
            let inner = analyze_properties(
                &key,
                pattern,
                seen,
                list_field(properties, "properties"),
                &properties["rest"],
            )?;
            let mut conditions = vec![Condition::InstanceOf {
                key,
                constructor: constructor_expression(&pattern["targetConstructor"]),
            }];
            conditions.extend(inner.conditions);
            Ok(Analysis {
                conditions,
                bindings: inner.bindings,
            })
        }
        Some("MatchOrPattern") => {
            let mut has_wildcard = false;
            let mut alternatives = Vec::new();
            for subpattern in list_field(pattern, "patterns") {
                let child = analyze_pattern(subpattern, key.clone(), seen)?;
                if !child.bindings.is_empty() {
                    return Err(refuse(
                        pattern,
                        "Bindings in match 'or' patterns are not yet supported.",
                    ));
                }
                if child.conditions.is_empty() {
                    has_wildcard = true;
                }
                alternatives.push(child.conditions);
            }
            if has_wildcard {
                return Ok(none);
            }
            Ok(Analysis {
                conditions: vec![Condition::Or { alternatives }],
                bindings: Vec::new(),
            })
        }
        other => Err(refuse(
            pattern,
            format!("unknown match pattern {:?}", other.unwrap_or("node")),
        )),
    }
}

fn expression_of_key(root: &Value, key: &Key) -> Value {
    key.iter().fold(root.clone(), |acc, prop| {
        let computed = node_type(prop) != Some("Identifier");
        member(acc, prop.clone(), computed)
    })
}

fn tests_of_condition(root: &Value, condition: &Condition) -> Vec<Value> {
    match condition {
        Condition::Eq { key, arg } => {
            vec![binary("===", expression_of_key(root, key), arg.clone())]
        }
        Condition::IsNan { key } => {
            vec![call(
                member(ident("Number"), ident("isNaN"), false),
                vec![expression_of_key(root, key)],
            )]
        }
        Condition::Array {
            key,
            length,
            at_least,
        } => {
            let is_array = call(
                member(ident("Array"), ident("isArray"), false),
                vec![expression_of_key(root, key)],
            );
            let operator = if *at_least { ">=" } else { "===" };
            let length_check = binary(
                operator,
                member(expression_of_key(root, key), ident("length"), false),
                number_literal(*length),
            );
            vec![is_array, length_check]
        }
        Condition::Object { key } => {
            let is_object = typeof_is(expression_of_key(root, key), "object");
            let not_null = binary("!==", expression_of_key(root, key), null_literal());
            let is_function = typeof_is(expression_of_key(root, key), "function");
            vec![disjunction(vec![
                conjunction(vec![is_object, not_null]),
                is_function,
            ])]
        }
        Condition::InstanceOf { key, constructor } => {
            vec![binary(
                "instanceof",
                expression_of_key(root, key),
                constructor.clone(),
            )]
        }
        Condition::PropExists { key, name } => {
            vec![binary(
                "in",
                string_literal(name),
                expression_of_key(root, key),
            )]
        }
        Condition::Or { alternatives } => {
            let tests = alternatives
                .iter()
                .map(|conditions| conjunction(tests_of_conditions(root, conditions)))
                .collect();
            vec![disjunction(tests)]
        }
    }
}

fn tests_of_conditions(root: &Value, conditions: &[Condition]) -> Vec<Value> {
    conditions
        .iter()
        .flat_map(|condition| tests_of_condition(root, condition))
        .collect()
}

fn statements_of_bindings(root: &Value, bindings: &[Binding], names: &mut GenId) -> Vec<Value> {
    bindings
        .iter()
        .map(|binding| match binding {
            Binding::Id { key, kind, id } => {
                variable_declaration(kind, id.clone(), expression_of_key(root, key))
            }
            Binding::ArrayRest {
                key,
                kind,
                id,
                exclude,
            } => {
                let init = call(
                    member(expression_of_key(root, key), ident("slice"), false),
                    vec![number_literal(*exclude)],
                );
                variable_declaration(kind, id.clone(), init)
            }
            Binding::ObjectRest {
                key,
                kind,
                id,
                exclude,
            } => {
                let mut properties: Vec<Value> = exclude
                    .iter()
                    .map(|prop| {
                        let computed = node_type(prop) != Some("Identifier");
                        property(prop.clone(), names.id(), computed, false)
                    })
                    .collect();
                properties.push(rest_element(id.clone()));
                let destructuring = json!({ "type": "ObjectPattern", "properties": properties });
                variable_declaration(kind, destructuring, expression_of_key(root, key))
            }
        })
        .collect()
}

const FALLTHROUGH_MESSAGE: &str = "Match: No case succesfully matched. Make exhaustive or add a wildcard case using '_'. Argument: ";

fn fallthrough_error(value: Value) -> Value {
    throw_statement(binary("+", string_literal(FALLTHROUGH_MESSAGE), value))
}

/// Whether evaluating `node` twice is harmless: an identifier, or a member
/// chain over one whose computed keys are literals.
fn is_simple_argument(node: &Value) -> bool {
    match node_type(node) {
        Some("Identifier" | "Super") => true,
        Some("MemberExpression") => {
            let computed = super::bool_field(node, "computed");
            if computed && node_type(&node["property"]) != Some("Literal") {
                return false;
            }
            is_simple_argument(&node["object"])
        }
        _ => false,
    }
}

struct CaseAnalysis {
    conditions: Vec<Condition>,
    bindings: Vec<Binding>,
    guard: Value,
    body: Value,
}

struct Cases {
    has_bindings: bool,
    has_wildcard: bool,
    analyses: Vec<CaseAnalysis>,
}

fn analyze_cases(cases: &[Value]) -> Result<Cases, TransformError> {
    let mut has_bindings = false;
    let mut has_wildcard = false;
    let mut analyses = Vec::new();
    for case in cases {
        let analysis = analyze_pattern(&case["pattern"], Vec::new(), &mut FxHashSet::default())?;
        has_bindings = has_bindings || !analysis.bindings.is_empty();
        let guard = case["guard"].clone();
        let catches_all = analysis.conditions.is_empty() && guard.is_null();
        analyses.push(CaseAnalysis {
            conditions: analysis.conditions,
            bindings: analysis.bindings,
            guard,
            body: case["body"].clone(),
        });
        if catches_all {
            has_wildcard = true;
            break;
        }
    }
    Ok(Cases {
        has_bindings,
        has_wildcard,
        analyses,
    })
}

fn map_match_expression(node: &mut Value, names: &mut GenId) -> Result<Value, TransformError> {
    let argument = take(node, "argument");
    let cases = take(node, "cases");
    let Cases {
        has_bindings,
        has_wildcard,
        mut analyses,
    } = analyze_cases(cases.as_array().map_or(&[], Vec::as_slice))?;

    let simple = !has_bindings && is_simple_argument(&argument);
    let generated_root = if simple { None } else { Some(names.id()) };
    let root = generated_root.clone().unwrap_or_else(|| argument.clone());

    if simple {
        let wildcard = if has_wildcard { analyses.pop() } else { None };
        let last = match wildcard {
            Some(analysis) => analysis.body,
            None => iife(
                vec![fallthrough_error(root.clone())],
                Vec::new(),
                Vec::new(),
            ),
        };
        return Ok(analyses.into_iter().rev().fold(last, |acc, analysis| {
            let mut tests = tests_of_conditions(&root, &analysis.conditions);
            if !analysis.guard.is_null() {
                tests.push(analysis.guard);
            }
            conditional(conjunction(tests), analysis.body, acc)
        }));
    }

    let mut statements: Vec<Value> = Vec::with_capacity(analyses.len() + 1);
    for analysis in analyses {
        let return_node = return_statement(analysis.body);
        let body_node = if analysis.guard.is_null() {
            return_node
        } else {
            if_statement(analysis.guard, return_node, None)
        };
        let binding_nodes = statements_of_bindings(&root, &analysis.bindings, names);
        let has_binding_nodes = !binding_nodes.is_empty();
        let mut case_body = binding_nodes;
        case_body.push(body_node);
        if !analysis.conditions.is_empty() {
            let tests = tests_of_conditions(&root, &analysis.conditions);
            statements.push(if_statement(conjunction(tests), block(case_body), None));
        } else if has_binding_nodes {
            statements.push(block(case_body));
        } else {
            statements.extend(case_body);
        }
    }
    if !has_wildcard {
        statements.push(fallthrough_error(root.clone()));
    }

    let (params, arguments) = match generated_root {
        Some(root) => (vec![root], vec![argument]),
        None => (Vec::new(), Vec::new()),
    };
    Ok(iife(statements, params, arguments))
}

fn map_match_statement(node: &mut Value, names: &mut GenId) -> Result<Value, TransformError> {
    let argument = take(node, "argument");
    let cases = take(node, "cases");
    let Cases {
        has_bindings,
        has_wildcard,
        analyses,
    } = analyze_cases(cases.as_array().map_or(&[], Vec::as_slice))?;

    let label = names.id();
    let simple = !has_bindings && is_simple_argument(&argument);
    let generated_root = if simple { None } else { Some(names.id()) };
    let root = generated_root.clone().unwrap_or_else(|| argument.clone());

    let mut statements = Vec::new();
    if let Some(generated) = generated_root {
        statements.push(variable_declaration("const", generated, argument));
    }
    for analysis in analyses {
        let break_node = json!({ "type": "BreakStatement", "label": label.clone() });
        let mut body_statements = list_field(&analysis.body, "body").to_vec();
        body_statements.push(break_node);
        let guarded = if analysis.guard.is_null() {
            body_statements
        } else {
            vec![if_statement(analysis.guard, block(body_statements), None)]
        };
        let mut case_body = statements_of_bindings(&root, &analysis.bindings, names);
        case_body.extend(guarded);
        if !analysis.conditions.is_empty() {
            let tests = tests_of_conditions(&root, &analysis.conditions);
            statements.push(if_statement(conjunction(tests), block(case_body), None));
        } else {
            statements.push(block(case_body));
        }
    }
    if !has_wildcard {
        statements.push(fallthrough_error(root));
    }

    Ok(json!({ "type": "LabeledStatement", "label": label, "body": block(statements) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;

    fn lowered(source: &str) -> Value {
        let mut program = parse(source).unwrap();
        lower(&mut program).unwrap();
        program
    }

    #[test]
    fn a_simple_argument_without_bindings_becomes_conditionals() {
        let program = lowered("const y = match (x) { \"a\" => 1, _ => 2 };\n");
        let init = &program["body"][0]["declarations"][0]["init"];
        assert_eq!(init["type"], "ConditionalExpression");
        assert_eq!(init["test"]["operator"], "===");
        assert_eq!(init["consequent"]["value"], 1);
        assert_eq!(init["alternate"]["value"], 2);
    }

    #[test]
    fn bindings_force_an_immediately_invoked_function() {
        let program = lowered("const y = match (x) { {kind: \"a\", v: const v} => v, _ => 0 };\n");
        let init = &program["body"][0]["declarations"][0]["init"];
        assert_eq!(init["type"], "CallExpression");
        assert_eq!(init["callee"]["type"], "ArrowFunctionExpression");
        assert_eq!(init["callee"]["params"][0]["name"], "$$gen$m0");
        let first = &init["callee"]["body"]["body"][0];
        assert_eq!(first["type"], "IfStatement");
        assert_eq!(
            first["consequent"]["body"][0]["type"],
            "VariableDeclaration"
        );
        assert_eq!(
            first["consequent"]["body"][0]["declarations"][0]["id"]["name"],
            "v"
        );
    }

    #[test]
    fn a_missing_wildcard_throws_at_the_end() {
        let program = lowered("const y = match (x) { 1 => \"one\" };\n");
        let init = &program["body"][0]["declarations"][0]["init"];
        assert_eq!(init["alternate"]["type"], "CallExpression");
        assert_eq!(
            init["alternate"]["callee"]["body"]["body"][0]["type"],
            "ThrowStatement"
        );
    }

    #[test]
    fn array_patterns_test_shape_and_slice_the_rest() {
        let program =
            lowered("const y = match (x) { [1, const b, ...const rest] => b, _ => 0 };\n");
        let body = &program["body"][0]["declarations"][0]["init"]["callee"]["body"]["body"][0];
        let test = &body["test"];
        assert_eq!(test["type"], "LogicalExpression");
        let bindings = body["consequent"]["body"].as_array().unwrap();
        assert_eq!(
            bindings[1]["declarations"][0]["init"]["callee"]["property"]["name"],
            "slice"
        );
    }

    #[test]
    fn a_match_statement_becomes_a_labelled_block() {
        let program = lowered("match (x) { 1 => { f(); } _ => { g(); } }\n");
        let labelled = &program["body"][0];
        assert_eq!(labelled["type"], "LabeledStatement");
        let body = labelled["body"]["body"].as_array().unwrap();
        assert_eq!(body[0]["type"], "IfStatement");
        let inner = body[0]["consequent"]["body"].as_array().unwrap();
        assert_eq!(inner.last().unwrap()["type"], "BreakStatement");
    }

    #[test]
    fn var_bindings_are_refused() {
        let mut program = parse("const y = match (x) { var v => v };\n").unwrap();
        let error = lower(&mut program).unwrap_err();
        assert!(
            matches!(error, TransformError::Lowering { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn generated_names_avoid_the_module_s_own() {
        let program = lowered("const $$gen$m0 = 1; const y = match (f()) { const v => v };\n");
        let init = &program["body"][1]["declarations"][0]["init"];
        assert_eq!(init["callee"]["params"][0]["name"], "$$gen$m1");
    }
}
