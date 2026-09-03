//! Babel AST → JavaScript text, with a source map.
//!
//! This printer exists for one reason: the React Compiler hands back an AST,
//! and something has to turn it into text again before oxc can lower the JSX
//! and generate the final module. It prints Babel's node vocabulary — the
//! subset that survives type erasure — readably but without pretence at
//! Prettier's layout; `uf fmt` is the formatter, this is the code generator.
//!
//! Two things it is careful about:
//!
//! * **Parentheses come from precedence, never from the input.** The compiler
//!   synthesises nodes with no notion of how the author bracketed them, so
//!   every operand is bracketed by comparing its precedence to its position.
//! * **Every node with a position becomes a mapping.** The map points at the
//!   Flow source the author wrote; nodes the compiler or the lowering passes
//!   invented carry no position and map to nothing, so a debugger lands on
//!   the author's line or nowhere, never on the wrong line.

use serde_json::Value;

use crate::TransformError;
use crate::lower::{bool_field, list_field, node_type, str_field};

/// One point in the generated text that came from one point in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    /// 0-based line in the generated text.
    pub generated_line: u32,
    /// 0-based column in the generated text, in UTF-16 code units.
    pub generated_column: u32,
    /// 0-based line in the source.
    pub original_line: u32,
    /// 0-based column in the source, in UTF-16 code units.
    pub original_column: u32,
}

/// Printed JavaScript and its mappings, in generated order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    /// The JavaScript, JSX included.
    pub code: String,
    /// Mappings sorted by generated position.
    pub mappings: Vec<Mapping>,
}

/// Print a Babel `File`.
///
/// # Errors
///
/// [`TransformError::Internal`] for a node kind the printer does not know,
/// which means a stage before it produced something outside the contract.
pub fn print(file: &Value) -> Result<Printed, TransformError> {
    let mut printer = Printer::default();
    printer.program(&file["program"])?;
    Ok(Printed {
        code: printer.out,
        mappings: printer.mappings,
    })
}

/// Operator precedence, higher binds tighter. Mirrors the ECMAScript grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Prec {
    Sequence = 0,
    Yield = 1,
    Assignment = 2,
    Conditional = 3,
    Coalesce = 4,
    LogicalOr = 5,
    LogicalAnd = 6,
    BitOr = 7,
    BitXor = 8,
    BitAnd = 9,
    Equality = 10,
    Relational = 11,
    Shift = 12,
    Additive = 13,
    Multiplicative = 14,
    Exponent = 15,
    Unary = 16,
    Update = 17,
    LeftHandSide = 18,
    Member = 19,
    Primary = 20,
}

fn binary_prec(operator: &str) -> Prec {
    match operator {
        "??" => Prec::Coalesce,
        "||" => Prec::LogicalOr,
        "&&" => Prec::LogicalAnd,
        "|" => Prec::BitOr,
        "^" => Prec::BitXor,
        "&" => Prec::BitAnd,
        "==" | "!=" | "===" | "!==" => Prec::Equality,
        "<" | ">" | "<=" | ">=" | "instanceof" | "in" => Prec::Relational,
        "<<" | ">>" | ">>>" => Prec::Shift,
        "+" | "-" => Prec::Additive,
        "*" | "/" | "%" => Prec::Multiplicative,
        "**" => Prec::Exponent,
        _ => Prec::Primary,
    }
}

fn prec_of(node: &Value) -> Prec {
    match node_type(node) {
        Some("SequenceExpression") => Prec::Sequence,
        Some("YieldExpression") => Prec::Yield,
        Some("AssignmentExpression" | "ArrowFunctionExpression" | "AssignmentPattern") => {
            Prec::Assignment
        }
        Some("ConditionalExpression") => Prec::Conditional,
        Some("BinaryExpression" | "LogicalExpression") => {
            binary_prec(str_field(node, "operator").unwrap_or(""))
        }
        Some("UnaryExpression" | "AwaitExpression") => Prec::Unary,
        Some("UpdateExpression") => Prec::Update,
        Some("CallExpression" | "OptionalCallExpression" | "NewExpression") => Prec::LeftHandSide,
        Some("MemberExpression" | "OptionalMemberExpression" | "TaggedTemplateExpression") => {
            Prec::Member
        }
        _ => Prec::Primary,
    }
}

#[derive(Default)]
struct Printer {
    out: String,
    line: u32,
    column: u32,
    indent: usize,
    mappings: Vec<Mapping>,
}

impl Printer {
    // ------------------------------------------------------------------
    // Output primitives
    // ------------------------------------------------------------------

    fn push(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += u32::try_from(ch.len_utf16()).unwrap_or(1);
            }
        }
        self.out.push_str(text);
    }

    fn newline(&mut self) {
        self.push("\n");
        for _ in 0..self.indent {
            self.push("  ");
        }
    }

    fn space(&mut self) {
        self.push(" ");
    }

    /// A word that must not fuse with what came before it.
    fn word(&mut self, text: &str) {
        if let Some(last) = self.out.chars().last()
            && (last.is_alphanumeric() || last == '_' || last == '$')
            && text
                .chars()
                .next()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$')
        {
            self.push(" ");
        }
        self.push(text);
    }

    fn mark(&mut self, node: &Value) {
        let Some(start) = node.get("loc").and_then(|loc| loc.get("start")) else {
            return;
        };
        let (Some(line), Some(column)) = (
            start.get("line").and_then(Value::as_u64),
            start.get("column").and_then(Value::as_u64),
        ) else {
            return;
        };
        if line == 0 {
            return;
        }
        self.mappings.push(Mapping {
            generated_line: self.line,
            generated_column: self.column,
            original_line: u32::try_from(line - 1).unwrap_or(u32::MAX),
            original_column: u32::try_from(column).unwrap_or(u32::MAX),
        });
    }

    fn unknown(node: &Value, context: &str) -> TransformError {
        TransformError::Internal(format!(
            "printer does not know {} in {context}",
            node_type(node).unwrap_or("a non-node")
        ))
    }

    // ------------------------------------------------------------------
    // Program and statements
    // ------------------------------------------------------------------

    fn program(&mut self, program: &Value) -> Result<(), TransformError> {
        self.directives(program)?;
        for statement in list_field(program, "body") {
            self.statement(statement)?;
            self.newline();
        }
        Ok(())
    }

    fn directives(&mut self, owner: &Value) -> Result<(), TransformError> {
        for directive in list_field(owner, "directives") {
            self.mark(directive);
            let literal = &directive["value"];
            match literal
                .get("extra")
                .and_then(|extra| extra.get("raw"))
                .and_then(Value::as_str)
            {
                Some(raw) => self.push(raw),
                None => self.string(str_field(literal, "value").unwrap_or("")),
            }
            self.push(";");
            self.newline();
        }
        Ok(())
    }

    fn block(&mut self, block: &Value) -> Result<(), TransformError> {
        self.push("{");
        let statements = list_field(block, "body");
        let directives = list_field(block, "directives");
        if statements.is_empty() && directives.is_empty() {
            self.push("}");
            return Ok(());
        }
        self.indent += 1;
        self.newline();
        self.directives(block)?;
        for (index, statement) in statements.iter().enumerate() {
            self.statement(statement)?;
            if index + 1 < statements.len() {
                self.newline();
            }
        }
        self.indent -= 1;
        self.newline();
        self.push("}");
        Ok(())
    }

    /// A statement in a position that takes one statement: braces if it is a
    /// block, otherwise indented on its own line.
    fn body(&mut self, statement: &Value) -> Result<(), TransformError> {
        if node_type(statement) == Some("BlockStatement") {
            self.space();
            return self.block(statement);
        }
        self.indent += 1;
        self.newline();
        self.statement(statement)?;
        self.indent -= 1;
        Ok(())
    }

    fn statement(&mut self, statement: &Value) -> Result<(), TransformError> {
        self.mark(statement);
        match node_type(statement) {
            Some("ExpressionStatement") => {
                let expression = &statement["expression"];
                let bracket = starts_statement_ambiguously(expression);
                if bracket {
                    self.push("(");
                }
                self.expression(expression, Prec::Sequence)?;
                if bracket {
                    self.push(")");
                }
                self.push(";");
            }
            Some("BlockStatement") => self.block(statement)?,
            Some("EmptyStatement") => self.push(";"),
            Some("DebuggerStatement") => self.push("debugger;"),
            Some("VariableDeclaration") => {
                self.variable_declaration(statement)?;
                self.push(";");
            }
            Some("FunctionDeclaration") => self.function(statement, true)?,
            Some("ClassDeclaration") => self.class(statement)?,
            Some("ReturnStatement") => {
                self.push("return");
                if !statement["argument"].is_null() {
                    self.space();
                    self.expression(&statement["argument"], Prec::Sequence)?;
                }
                self.push(";");
            }
            Some("ThrowStatement") => {
                self.push("throw ");
                self.expression(&statement["argument"], Prec::Sequence)?;
                self.push(";");
            }
            Some("IfStatement") => {
                self.push("if (");
                self.expression(&statement["test"], Prec::Sequence)?;
                self.push(")");
                self.body(&statement["consequent"])?;
                if !statement["alternate"].is_null() {
                    if node_type(&statement["consequent"]) == Some("BlockStatement") {
                        self.space();
                    } else {
                        self.newline();
                    }
                    self.push("else");
                    if node_type(&statement["alternate"]) == Some("IfStatement") {
                        self.space();
                        self.statement(&statement["alternate"])?;
                    } else {
                        self.body(&statement["alternate"])?;
                    }
                }
            }
            Some("ForStatement") => {
                self.push("for (");
                let init = &statement["init"];
                if node_type(init) == Some("VariableDeclaration") {
                    self.variable_declaration(init)?;
                } else if !init.is_null() {
                    self.expression(init, Prec::Sequence)?;
                }
                self.push(";");
                if !statement["test"].is_null() {
                    self.space();
                    self.expression(&statement["test"], Prec::Sequence)?;
                }
                self.push(";");
                if !statement["update"].is_null() {
                    self.space();
                    self.expression(&statement["update"], Prec::Sequence)?;
                }
                self.push(")");
                self.body(&statement["body"])?;
            }
            Some("ForInStatement" | "ForOfStatement") => {
                self.push("for");
                if bool_field(statement, "await") {
                    self.push(" await");
                }
                self.push(" (");
                let left = &statement["left"];
                if node_type(left) == Some("VariableDeclaration") {
                    self.variable_declaration(left)?;
                } else {
                    self.pattern(left)?;
                }
                self.push(if node_type(statement) == Some("ForInStatement") {
                    " in "
                } else {
                    " of "
                });
                self.expression(&statement["right"], Prec::Assignment)?;
                self.push(")");
                self.body(&statement["body"])?;
            }
            Some("WhileStatement") => {
                self.push("while (");
                self.expression(&statement["test"], Prec::Sequence)?;
                self.push(")");
                self.body(&statement["body"])?;
            }
            Some("DoWhileStatement") => {
                self.push("do");
                self.body(&statement["body"])?;
                if node_type(&statement["body"]) == Some("BlockStatement") {
                    self.space();
                } else {
                    self.newline();
                }
                self.push("while (");
                self.expression(&statement["test"], Prec::Sequence)?;
                self.push(");");
            }
            Some("BreakStatement" | "ContinueStatement") => {
                self.push(if node_type(statement) == Some("BreakStatement") {
                    "break"
                } else {
                    "continue"
                });
                if !statement["label"].is_null() {
                    self.space();
                    self.identifier(&statement["label"]);
                }
                self.push(";");
            }
            Some("LabeledStatement") => {
                self.identifier(&statement["label"]);
                self.push(": ");
                self.statement(&statement["body"])?;
            }
            Some("WithStatement") => {
                self.push("with (");
                self.expression(&statement["object"], Prec::Sequence)?;
                self.push(")");
                self.body(&statement["body"])?;
            }
            Some("SwitchStatement") => {
                self.push("switch (");
                self.expression(&statement["discriminant"], Prec::Sequence)?;
                self.push(") {");
                self.indent += 1;
                for case in list_field(statement, "cases") {
                    self.newline();
                    self.mark(case);
                    if case["test"].is_null() {
                        self.push("default:");
                    } else {
                        self.push("case ");
                        self.expression(&case["test"], Prec::Sequence)?;
                        self.push(":");
                    }
                    self.indent += 1;
                    for consequent in list_field(case, "consequent") {
                        self.newline();
                        self.statement(consequent)?;
                    }
                    self.indent -= 1;
                }
                self.indent -= 1;
                self.newline();
                self.push("}");
            }
            Some("TryStatement") => {
                self.push("try ");
                self.block(&statement["block"])?;
                let handler = &statement["handler"];
                if !handler.is_null() {
                    self.push(" catch");
                    if !handler["param"].is_null() {
                        self.push(" (");
                        self.pattern(&handler["param"])?;
                        self.push(")");
                    }
                    self.space();
                    self.block(&handler["body"])?;
                }
                if !statement["finalizer"].is_null() {
                    self.push(" finally ");
                    self.block(&statement["finalizer"])?;
                }
            }
            Some("ImportDeclaration") => self.import_declaration(statement)?,
            Some("ExportNamedDeclaration") => self.export_named(statement)?,
            Some("ExportDefaultDeclaration") => {
                self.push("export default ");
                let declaration = &statement["declaration"];
                match node_type(declaration) {
                    Some("FunctionDeclaration") => self.function(declaration, true)?,
                    Some("ClassDeclaration") => self.class(declaration)?,
                    _ => {
                        let bracket = matches!(node_type(declaration), Some("SequenceExpression"))
                            || starts_statement_ambiguously(declaration);
                        if bracket {
                            self.push("(");
                        }
                        self.expression(declaration, Prec::Assignment)?;
                        if bracket {
                            self.push(")");
                        }
                        self.push(";");
                    }
                }
            }
            Some("ExportAllDeclaration") => {
                self.push("export * from ");
                self.expression(&statement["source"], Prec::Primary)?;
                self.import_attributes(statement)?;
                self.push(";");
            }
            Some("StaticBlock") => {
                self.push("static ");
                self.block(statement)?;
            }
            _ => return Err(Self::unknown(statement, "statement position")),
        }
        Ok(())
    }

    fn variable_declaration(&mut self, declaration: &Value) -> Result<(), TransformError> {
        self.mark(declaration);
        self.push(str_field(declaration, "kind").unwrap_or("let"));
        self.space();
        for (index, declarator) in list_field(declaration, "declarations").iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.mark(declarator);
            self.pattern(&declarator["id"])?;
            if !declarator["init"].is_null() {
                self.push(" = ");
                self.expression(&declarator["init"], Prec::Assignment)?;
            }
        }
        Ok(())
    }

    fn import_declaration(&mut self, declaration: &Value) -> Result<(), TransformError> {
        self.push("import");
        let specifiers = list_field(declaration, "specifiers");
        if specifiers.is_empty() {
            self.space();
            self.expression(&declaration["source"], Prec::Primary)?;
            self.import_attributes(declaration)?;
            self.push(";");
            return Ok(());
        }
        self.space();
        let mut named_open = false;
        let mut first = true;
        for specifier in specifiers {
            match node_type(specifier) {
                Some("ImportDefaultSpecifier") => {
                    if !first {
                        self.push(", ");
                    }
                    self.identifier(&specifier["local"]);
                }
                Some("ImportNamespaceSpecifier") => {
                    if !first {
                        self.push(", ");
                    }
                    self.push("* as ");
                    self.identifier(&specifier["local"]);
                }
                _ => {
                    if !named_open {
                        if !first {
                            self.push(", ");
                        }
                        self.push("{ ");
                        named_open = true;
                    } else {
                        self.push(", ");
                    }
                    let imported = &specifier["imported"];
                    let local = &specifier["local"];
                    self.module_export_name(imported)?;
                    if str_field(imported, "name").or_else(|| str_field(imported, "value"))
                        != str_field(local, "name")
                    {
                        self.push(" as ");
                        self.identifier(local);
                    }
                }
            }
            first = false;
        }
        if named_open {
            self.push(" }");
        }
        self.push(" from ");
        self.expression(&declaration["source"], Prec::Primary)?;
        self.import_attributes(declaration)?;
        self.push(";");
        Ok(())
    }

    fn import_attributes(&mut self, declaration: &Value) -> Result<(), TransformError> {
        let attributes = if !list_field(declaration, "attributes").is_empty() {
            list_field(declaration, "attributes")
        } else {
            list_field(declaration, "assertions")
        };
        if attributes.is_empty() {
            return Ok(());
        }
        self.push(" with { ");
        for (index, attribute) in attributes.iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.property_key(&attribute["key"], false)?;
            self.push(": ");
            self.expression(&attribute["value"], Prec::Primary)?;
        }
        self.push(" }");
        Ok(())
    }

    fn export_named(&mut self, declaration: &Value) -> Result<(), TransformError> {
        self.push("export ");
        let inner = &declaration["declaration"];
        if !inner.is_null() {
            match node_type(inner) {
                Some("VariableDeclaration") => {
                    self.variable_declaration(inner)?;
                    self.push(";");
                }
                Some("FunctionDeclaration") => self.function(inner, true)?,
                Some("ClassDeclaration") => self.class(inner)?,
                _ => return Err(Self::unknown(inner, "export declaration")),
            }
            return Ok(());
        }
        let specifiers = list_field(declaration, "specifiers");
        let namespace = specifiers
            .iter()
            .find(|s| node_type(s) == Some("ExportNamespaceSpecifier"));
        if let Some(namespace) = namespace {
            self.push("* as ");
            self.module_export_name(&namespace["exported"])?;
        } else {
            self.push("{ ");
            for (index, specifier) in specifiers.iter().enumerate() {
                if index > 0 {
                    self.push(", ");
                }
                let local = &specifier["local"];
                let exported = &specifier["exported"];
                self.module_export_name(local)?;
                if str_field(local, "name").or_else(|| str_field(local, "value"))
                    != str_field(exported, "name").or_else(|| str_field(exported, "value"))
                {
                    self.push(" as ");
                    self.module_export_name(exported)?;
                }
            }
            self.push(" }");
        }
        if !declaration["source"].is_null() {
            self.push(" from ");
            self.expression(&declaration["source"], Prec::Primary)?;
            self.import_attributes(declaration)?;
        }
        self.push(";");
        Ok(())
    }

    fn module_export_name(&mut self, name: &Value) -> Result<(), TransformError> {
        match node_type(name) {
            Some("Identifier") => {
                self.identifier(name);
                Ok(())
            }
            Some("StringLiteral") => self.expression(name, Prec::Primary),
            _ => Err(Self::unknown(name, "module export name")),
        }
    }

    // ------------------------------------------------------------------
    // Functions and classes
    // ------------------------------------------------------------------

    fn function(&mut self, function: &Value, declaration: bool) -> Result<(), TransformError> {
        self.mark(function);
        if bool_field(function, "async") {
            self.word("async");
            self.space();
        }
        self.word("function");
        if bool_field(function, "generator") {
            self.push("*");
        }
        if !function["id"].is_null() {
            self.space();
            self.identifier(&function["id"]);
        } else if !declaration {
            // `function () {}`, the way Prettier spells an anonymous one.
            self.space();
        }
        self.params(function)?;
        self.space();
        self.block(&function["body"])?;
        Ok(())
    }

    fn params(&mut self, function: &Value) -> Result<(), TransformError> {
        self.push("(");
        for (index, param) in list_field(function, "params").iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.pattern(param)?;
        }
        self.push(")");
        Ok(())
    }

    fn arrow(&mut self, arrow: &Value) -> Result<(), TransformError> {
        self.mark(arrow);
        if bool_field(arrow, "async") {
            self.word("async");
            self.space();
        }
        self.params(arrow)?;
        self.push(" => ");
        let body = &arrow["body"];
        if node_type(body) == Some("BlockStatement") {
            return self.block(body);
        }
        let bracket = matches!(
            node_type(body),
            Some("ObjectExpression" | "SequenceExpression")
        ) || starts_with_object(body);
        if bracket {
            self.push("(");
        }
        self.expression(body, Prec::Assignment)?;
        if bracket {
            self.push(")");
        }
        Ok(())
    }

    fn class(&mut self, class: &Value) -> Result<(), TransformError> {
        self.mark(class);
        self.word("class");
        if !class["id"].is_null() {
            self.space();
            self.identifier(&class["id"]);
        }
        if !class["superClass"].is_null() {
            self.push(" extends ");
            self.expression(&class["superClass"], Prec::LeftHandSide)?;
        }
        self.push(" {");
        let members = list_field(&class["body"], "body");
        if members.is_empty() {
            self.push("}");
            return Ok(());
        }
        self.indent += 1;
        for member in members {
            self.newline();
            self.class_member(member)?;
        }
        self.indent -= 1;
        self.newline();
        self.push("}");
        Ok(())
    }

    fn class_member(&mut self, member: &Value) -> Result<(), TransformError> {
        self.mark(member);
        match node_type(member) {
            Some("ClassMethod" | "ClassPrivateMethod") => {
                if bool_field(member, "static") {
                    self.push("static ");
                }
                self.method_head(member)?;
                self.params(member)?;
                self.space();
                self.block(&member["body"])
            }
            Some("ClassProperty" | "ClassPrivateProperty") => {
                if bool_field(member, "static") {
                    self.push("static ");
                }
                self.property_key(&member["key"], bool_field(member, "computed"))?;
                if !member["value"].is_null() {
                    self.push(" = ");
                    self.expression(&member["value"], Prec::Assignment)?;
                }
                self.push(";");
                Ok(())
            }
            Some("StaticBlock") => {
                self.push("static ");
                self.block(member)
            }
            _ => Err(Self::unknown(member, "class body")),
        }
    }

    /// `async *get [key]` — everything before a method's parameter list.
    fn method_head(&mut self, method: &Value) -> Result<(), TransformError> {
        if bool_field(method, "async") {
            self.push("async ");
        }
        match str_field(method, "kind") {
            Some("get") => self.push("get "),
            Some("set") => self.push("set "),
            _ => {}
        }
        if bool_field(method, "generator") {
            self.push("*");
        }
        self.property_key(&method["key"], bool_field(method, "computed"))
    }

    fn property_key(&mut self, key: &Value, computed: bool) -> Result<(), TransformError> {
        if computed {
            self.push("[");
            self.expression(key, Prec::Assignment)?;
            self.push("]");
            return Ok(());
        }
        match node_type(key) {
            Some("Identifier") => {
                self.identifier(key);
                Ok(())
            }
            Some("PrivateName") => {
                self.mark(key);
                self.push("#");
                self.push(str_field(&key["id"], "name").unwrap_or(""));
                Ok(())
            }
            Some("StringLiteral" | "NumericLiteral" | "BigIntLiteral") => {
                self.expression(key, Prec::Primary)
            }
            _ => Err(Self::unknown(key, "property key")),
        }
    }

    // ------------------------------------------------------------------
    // Patterns
    // ------------------------------------------------------------------

    fn pattern(&mut self, pattern: &Value) -> Result<(), TransformError> {
        self.mark(pattern);
        match node_type(pattern) {
            Some("Identifier") => {
                self.identifier(pattern);
                Ok(())
            }
            Some("ObjectPattern") => {
                self.push("{");
                let properties = list_field(pattern, "properties");
                for (index, property) in properties.iter().enumerate() {
                    self.push(if index == 0 { " " } else { ", " });
                    match node_type(property) {
                        Some("RestElement") => {
                            self.push("...");
                            self.pattern(&property["argument"])?;
                        }
                        _ => {
                            let computed = bool_field(property, "computed");
                            if bool_field(property, "shorthand") && !computed {
                                self.pattern(&property["value"])?;
                            } else {
                                self.property_key(&property["key"], computed)?;
                                self.push(": ");
                                self.pattern(&property["value"])?;
                            }
                        }
                    }
                }
                if !properties.is_empty() {
                    self.space();
                }
                self.push("}");
                Ok(())
            }
            Some("ArrayPattern") => {
                self.push("[");
                let elements = list_field(pattern, "elements");
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        self.push(", ");
                    }
                    if !element.is_null() {
                        self.pattern(element)?;
                    }
                }
                if elements.last().is_some_and(Value::is_null) {
                    self.push(",");
                }
                self.push("]");
                Ok(())
            }
            Some("AssignmentPattern") => {
                self.pattern(&pattern["left"])?;
                self.push(" = ");
                self.expression(&pattern["right"], Prec::Assignment)
            }
            Some("RestElement") => {
                self.push("...");
                self.pattern(&pattern["argument"])
            }
            Some("MemberExpression" | "OptionalMemberExpression") => {
                self.expression(pattern, Prec::Member)
            }
            _ => Err(Self::unknown(pattern, "pattern position")),
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn identifier(&mut self, identifier: &Value) {
        self.mark(identifier);
        self.word(str_field(identifier, "name").unwrap_or(""));
    }

    fn string(&mut self, value: &str) {
        self.push(&serde_json::to_string(value).unwrap_or_else(|_| String::from("\"\"")));
    }

    /// Print `node` inside brackets this caller decided on, so the node's own
    /// precedence does not add a second pair.
    fn bracketed(&mut self, node: &Value) -> Result<(), TransformError> {
        self.push("(");
        self.expression(node, Prec::Sequence)?;
        self.push(")");
        Ok(())
    }

    /// Print `node` as an expression in a position that requires at least
    /// `required` precedence, bracketing it when it binds looser.
    fn expression(&mut self, node: &Value, required: Prec) -> Result<(), TransformError> {
        let own = prec_of(node);
        let bracket = own < required || needs_brackets_in_chain(node, required);
        if bracket {
            self.push("(");
        }
        self.expression_inner(node)?;
        if bracket {
            self.push(")");
        }
        Ok(())
    }

    fn expression_inner(&mut self, node: &Value) -> Result<(), TransformError> {
        self.mark(node);
        match node_type(node) {
            Some("Identifier") => {
                self.word(str_field(node, "name").unwrap_or(""));
            }
            Some("StringLiteral") => match node
                .get("extra")
                .and_then(|e| e.get("raw"))
                .and_then(Value::as_str)
            {
                Some(raw) => self.push(raw),
                None => self.string(str_field(node, "value").unwrap_or("")),
            },
            Some("NumericLiteral") => match node
                .get("extra")
                .and_then(|e| e.get("raw"))
                .and_then(Value::as_str)
            {
                Some(raw) => self.word(raw),
                None => {
                    let value = node["value"].as_f64().unwrap_or(0.0);
                    self.word(&format_number(value));
                }
            },
            Some("BigIntLiteral") => {
                let text = str_field(node, "value").unwrap_or("0");
                self.word(&format!("{}n", text.trim_end_matches('n')));
            }
            Some("BooleanLiteral") => self.word(if bool_field(node, "value") {
                "true"
            } else {
                "false"
            }),
            Some("NullLiteral") => self.word("null"),
            Some("RegExpLiteral") => {
                self.push("/");
                self.push(str_field(node, "pattern").unwrap_or(""));
                self.push("/");
                self.push(str_field(node, "flags").unwrap_or(""));
            }
            Some("ThisExpression") => self.word("this"),
            Some("Super") => self.word("super"),
            Some("Import") => self.word("import"),
            Some("MetaProperty") => {
                self.identifier(&node["meta"]);
                self.push(".");
                self.identifier(&node["property"]);
            }
            Some("TemplateLiteral") => self.template(node)?,
            Some("TaggedTemplateExpression") => {
                self.expression(&node["tag"], Prec::Member)?;
                self.template(&node["quasi"])?;
            }
            Some("ArrayExpression") => {
                self.push("[");
                let elements = list_field(node, "elements");
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        self.push(", ");
                    }
                    if !element.is_null() {
                        self.expression(element, Prec::Assignment)?;
                    }
                }
                if elements.last().is_some_and(Value::is_null) {
                    self.push(",");
                }
                self.push("]");
            }
            Some("ObjectExpression") => self.object(node)?,
            Some("FunctionExpression") => self.function(node, false)?,
            Some("ArrowFunctionExpression") => self.arrow(node)?,
            Some("ClassExpression") => self.class(node)?,
            Some("SpreadElement") => {
                self.push("...");
                self.expression(&node["argument"], Prec::Assignment)?;
            }
            Some("UnaryExpression") => {
                let operator = str_field(node, "operator").unwrap_or("");
                self.word(operator);
                let argument = &node["argument"];
                let same_sign = matches!(operator, "+" | "-")
                    && matches!(
                        node_type(argument),
                        Some("UnaryExpression" | "UpdateExpression")
                    )
                    && str_field(argument, "operator")
                        .is_some_and(|inner| inner.starts_with(operator));
                if operator.chars().all(char::is_alphabetic) || same_sign {
                    self.space();
                }
                self.expression(argument, Prec::Unary)?;
            }
            Some("UpdateExpression") => {
                let operator = str_field(node, "operator").unwrap_or("++");
                if bool_field(node, "prefix") {
                    self.push(operator);
                    self.expression(&node["argument"], Prec::Unary)?;
                } else {
                    self.expression(&node["argument"], Prec::LeftHandSide)?;
                    self.push(operator);
                }
            }
            Some("AwaitExpression") => {
                self.word("await");
                self.space();
                self.expression(&node["argument"], Prec::Unary)?;
            }
            Some("YieldExpression") => {
                self.word("yield");
                if bool_field(node, "delegate") {
                    self.push("*");
                }
                if !node["argument"].is_null() {
                    self.space();
                    self.expression(&node["argument"], Prec::Assignment)?;
                }
            }
            Some("BinaryExpression" | "LogicalExpression") => self.binary(node)?,
            Some("AssignmentExpression") => {
                let left = &node["left"];
                if matches!(node_type(left), Some("ObjectPattern" | "ArrayPattern")) {
                    self.pattern(left)?;
                } else {
                    self.expression(left, Prec::LeftHandSide)?;
                }
                self.space();
                self.push(str_field(node, "operator").unwrap_or("="));
                self.space();
                self.expression(&node["right"], Prec::Assignment)?;
            }
            Some("AssignmentPattern") => self.pattern(node)?,
            Some("ConditionalExpression") => {
                self.expression(&node["test"], Prec::Coalesce)?;
                self.push(" ? ");
                self.expression(&node["consequent"], Prec::Assignment)?;
                self.push(" : ");
                self.expression(&node["alternate"], Prec::Assignment)?;
            }
            Some("SequenceExpression") => {
                for (index, expression) in list_field(node, "expressions").iter().enumerate() {
                    if index > 0 {
                        self.push(", ");
                    }
                    self.expression(expression, Prec::Assignment)?;
                }
            }
            Some("CallExpression" | "OptionalCallExpression") => {
                let callee = &node["callee"];
                let bracket = matches!(
                    node_type(callee),
                    Some("FunctionExpression" | "ClassExpression")
                ) || (node_type(node) == Some("CallExpression")
                    && matches!(
                        node_type(callee),
                        Some("OptionalMemberExpression" | "OptionalCallExpression")
                    ));
                if bracket {
                    self.bracketed(callee)?;
                } else {
                    self.expression(callee, Prec::LeftHandSide)?;
                }
                if bool_field(node, "optional") {
                    self.push("?.");
                }
                self.arguments(node)?;
            }
            Some("NewExpression") => {
                self.word("new");
                self.space();
                let callee = &node["callee"];
                if contains_call(callee) {
                    self.bracketed(callee)?;
                } else {
                    self.expression(callee, Prec::Member)?;
                }
                self.arguments(node)?;
            }
            Some("MemberExpression" | "OptionalMemberExpression") => {
                let object = &node["object"];
                let bracket = matches!(node_type(object), Some("NumericLiteral"))
                    || (node_type(node) == Some("MemberExpression")
                        && matches!(
                            node_type(object),
                            Some("OptionalMemberExpression" | "OptionalCallExpression")
                        ));
                if bracket {
                    self.bracketed(object)?;
                } else {
                    self.expression(object, Prec::Member)?;
                }
                let optional = bool_field(node, "optional");
                if bool_field(node, "computed") {
                    self.push(if optional { "?.[" } else { "[" });
                    self.expression(&node["property"], Prec::Sequence)?;
                    self.push("]");
                } else {
                    self.push(if optional { "?." } else { "." });
                    let property = &node["property"];
                    if node_type(property) == Some("PrivateName") {
                        self.property_key(property, false)?;
                    } else {
                        self.mark(property);
                        self.push(str_field(property, "name").unwrap_or(""));
                    }
                }
            }
            Some("ParenthesizedExpression") => {
                self.push("(");
                self.expression(&node["expression"], Prec::Sequence)?;
                self.push(")");
            }
            Some("JSXElement") => self.jsx_element(node)?,
            Some("JSXFragment") => self.jsx_fragment(node)?,
            Some("ObjectPattern" | "ArrayPattern" | "RestElement") => self.pattern(node)?,
            _ => return Err(Self::unknown(node, "expression position")),
        }
        Ok(())
    }

    fn binary(&mut self, node: &Value) -> Result<(), TransformError> {
        let operator = str_field(node, "operator").unwrap_or("+");
        let own = binary_prec(operator);
        let left = &node["left"];
        let right = &node["right"];
        // `**` is right-associative and refuses a unary operand on its left;
        // everything else is left-associative.
        let (left_required, right_required) = if operator == "**" {
            (Prec::Update, own)
        } else {
            (own, higher(own))
        };
        let mix = |operand: &Value| {
            // `a ?? b || c` is a syntax error without brackets.
            matches!(node_type(operand), Some("LogicalExpression"))
                && ((operator == "??") != (str_field(operand, "operator") == Some("??")))
        };
        if mix(left) {
            self.bracketed(left)?;
        } else {
            self.expression(left, left_required)?;
        }
        self.space();
        self.word(operator);
        self.space();
        if mix(right) {
            self.bracketed(right)?;
        } else {
            self.expression(right, right_required)?;
        }
        Ok(())
    }

    fn arguments(&mut self, node: &Value) -> Result<(), TransformError> {
        self.push("(");
        for (index, argument) in list_field(node, "arguments").iter().enumerate() {
            if index > 0 {
                self.push(", ");
            }
            self.expression(argument, Prec::Assignment)?;
        }
        self.push(")");
        Ok(())
    }

    fn object(&mut self, object: &Value) -> Result<(), TransformError> {
        let properties = list_field(object, "properties");
        if properties.is_empty() {
            self.push("{}");
            return Ok(());
        }
        self.push("{");
        self.indent += 1;
        for (index, property) in properties.iter().enumerate() {
            self.newline();
            self.mark(property);
            match node_type(property) {
                Some("SpreadElement") => {
                    self.push("...");
                    self.expression(&property["argument"], Prec::Assignment)?;
                }
                Some("ObjectMethod") => {
                    self.method_head(property)?;
                    self.params(property)?;
                    self.space();
                    self.block(&property["body"])?;
                }
                Some("ObjectProperty") => {
                    let computed = bool_field(property, "computed");
                    let value = &property["value"];
                    if bool_field(property, "shorthand")
                        && !computed
                        && node_type(value) == Some("Identifier")
                        && str_field(value, "name") == str_field(&property["key"], "name")
                    {
                        self.identifier(value);
                    } else {
                        self.property_key(&property["key"], computed)?;
                        self.push(": ");
                        self.expression(value, Prec::Assignment)?;
                    }
                }
                _ => return Err(Self::unknown(property, "object literal")),
            }
            if index + 1 < properties.len() {
                self.push(",");
            }
        }
        self.indent -= 1;
        self.newline();
        self.push("}");
        Ok(())
    }

    fn template(&mut self, template: &Value) -> Result<(), TransformError> {
        self.push("`");
        let quasis = list_field(template, "quasis");
        let expressions = list_field(template, "expressions");
        for (index, quasi) in quasis.iter().enumerate() {
            let raw = quasi
                .get("value")
                .and_then(|value| value.get("raw"))
                .and_then(Value::as_str)
                .unwrap_or("");
            self.push(raw);
            if let Some(expression) = expressions.get(index) {
                self.push("${");
                self.expression(expression, Prec::Sequence)?;
                self.push("}");
            }
        }
        self.push("`");
        Ok(())
    }

    // ------------------------------------------------------------------
    // JSX
    // ------------------------------------------------------------------

    fn jsx_name(&mut self, name: &Value) -> Result<(), TransformError> {
        match node_type(name) {
            Some("JSXIdentifier") => {
                self.mark(name);
                self.push(str_field(name, "name").unwrap_or(""));
                Ok(())
            }
            Some("JSXMemberExpression") => {
                self.jsx_name(&name["object"])?;
                self.push(".");
                self.jsx_name(&name["property"])
            }
            Some("JSXNamespacedName") => {
                self.jsx_name(&name["namespace"])?;
                self.push(":");
                self.jsx_name(&name["name"])
            }
            _ => Err(Self::unknown(name, "JSX name")),
        }
    }

    fn jsx_element(&mut self, element: &Value) -> Result<(), TransformError> {
        let opening = &element["openingElement"];
        self.push("<");
        self.jsx_name(&opening["name"])?;
        for attribute in list_field(opening, "attributes") {
            self.space();
            self.mark(attribute);
            match node_type(attribute) {
                Some("JSXSpreadAttribute") => {
                    self.push("{...");
                    self.expression(&attribute["argument"], Prec::Assignment)?;
                    self.push("}");
                }
                _ => {
                    self.jsx_name(&attribute["name"])?;
                    let value = &attribute["value"];
                    if value.is_null() {
                        continue;
                    }
                    self.push("=");
                    match node_type(value) {
                        Some("StringLiteral") => match value
                            .get("extra")
                            .and_then(|e| e.get("raw"))
                            .and_then(Value::as_str)
                        {
                            Some(raw) => self.push(raw),
                            None => {
                                self.push("\"");
                                self.push(
                                    &str_field(value, "value")
                                        .unwrap_or("")
                                        .replace('"', "&quot;"),
                                );
                                self.push("\"");
                            }
                        },
                        Some("JSXExpressionContainer") => self.jsx_container(value)?,
                        Some("JSXElement") => self.jsx_element(value)?,
                        Some("JSXFragment") => self.jsx_fragment(value)?,
                        _ => return Err(Self::unknown(value, "JSX attribute value")),
                    }
                }
            }
        }
        if bool_field(opening, "selfClosing") || element["closingElement"].is_null() {
            self.push(" />");
            return Ok(());
        }
        self.push(">");
        self.jsx_children(element)?;
        self.push("</");
        self.jsx_name(&opening["name"])?;
        self.push(">");
        Ok(())
    }

    fn jsx_fragment(&mut self, fragment: &Value) -> Result<(), TransformError> {
        self.push("<>");
        self.jsx_children(fragment)?;
        self.push("</>");
        Ok(())
    }

    fn jsx_children(&mut self, parent: &Value) -> Result<(), TransformError> {
        for child in list_field(parent, "children") {
            match node_type(child) {
                Some("JSXText") => {
                    let raw = child
                        .get("extra")
                        .and_then(|e| e.get("raw"))
                        .and_then(Value::as_str)
                        .or_else(|| str_field(child, "value"))
                        .unwrap_or("");
                    self.push(raw);
                }
                Some("JSXExpressionContainer") => self.jsx_container(child)?,
                Some("JSXSpreadChild") => {
                    self.push("{...");
                    self.expression(&child["expression"], Prec::Assignment)?;
                    self.push("}");
                }
                Some("JSXElement") => self.jsx_element(child)?,
                Some("JSXFragment") => self.jsx_fragment(child)?,
                _ => return Err(Self::unknown(child, "JSX children")),
            }
        }
        Ok(())
    }

    fn jsx_container(&mut self, container: &Value) -> Result<(), TransformError> {
        self.push("{");
        let expression = &container["expression"];
        if node_type(expression) != Some("JSXEmptyExpression") {
            self.expression(expression, Prec::Sequence)?;
        }
        self.push("}");
        Ok(())
    }
}

/// The precedence one step tighter than `prec`, for a left-associative
/// operator's right operand.
fn higher(prec: Prec) -> Prec {
    match prec {
        Prec::Sequence => Prec::Yield,
        Prec::Yield => Prec::Assignment,
        Prec::Assignment => Prec::Conditional,
        Prec::Conditional => Prec::Coalesce,
        Prec::Coalesce => Prec::LogicalOr,
        Prec::LogicalOr => Prec::LogicalAnd,
        Prec::LogicalAnd => Prec::BitOr,
        Prec::BitOr => Prec::BitXor,
        Prec::BitXor => Prec::BitAnd,
        Prec::BitAnd => Prec::Equality,
        Prec::Equality => Prec::Relational,
        Prec::Relational => Prec::Shift,
        Prec::Shift => Prec::Additive,
        Prec::Additive => Prec::Multiplicative,
        Prec::Multiplicative => Prec::Exponent,
        Prec::Exponent => Prec::Unary,
        Prec::Unary => Prec::Update,
        Prec::Update => Prec::LeftHandSide,
        Prec::LeftHandSide => Prec::Member,
        Prec::Member | Prec::Primary => Prec::Primary,
    }
}

/// Arrow functions, function expressions and classes in a member or call
/// position need brackets even though their own precedence says "primary".
fn needs_brackets_in_chain(node: &Value, required: Prec) -> bool {
    required >= Prec::LeftHandSide
        && matches!(
            node_type(node),
            Some("ArrowFunctionExpression" | "FunctionExpression" | "ClassExpression")
        )
        && required > Prec::LeftHandSide
}

/// Whether an expression statement would be read as a block, a function
/// declaration, a class declaration or a `let` binding without brackets.
fn starts_statement_ambiguously(expression: &Value) -> bool {
    match node_type(expression) {
        Some("ObjectExpression" | "FunctionExpression" | "ClassExpression") => true,
        Some("AssignmentExpression") if node_type(&expression["left"]) == Some("ObjectPattern") => {
            true
        }
        Some("Identifier") => str_field(expression, "name") == Some("let"),
        Some("MemberExpression" | "OptionalMemberExpression") => {
            starts_statement_ambiguously(&expression["object"])
        }
        Some("CallExpression" | "OptionalCallExpression") => {
            starts_statement_ambiguously(&expression["callee"])
        }
        Some("TaggedTemplateExpression") => starts_statement_ambiguously(&expression["tag"]),
        Some("BinaryExpression" | "LogicalExpression" | "AssignmentExpression") => {
            starts_statement_ambiguously(&expression["left"])
        }
        Some("ConditionalExpression") => starts_statement_ambiguously(&expression["test"]),
        Some("SequenceExpression") => list_field(expression, "expressions")
            .first()
            .is_some_and(starts_statement_ambiguously),
        Some("UpdateExpression") if !bool_field(expression, "prefix") => {
            starts_statement_ambiguously(&expression["argument"])
        }
        _ => false,
    }
}

/// Whether an arrow body would be read as a block: it starts with `{`.
fn starts_with_object(expression: &Value) -> bool {
    match node_type(expression) {
        Some("ObjectExpression") => true,
        Some("MemberExpression" | "OptionalMemberExpression") => {
            starts_with_object(&expression["object"])
        }
        Some("CallExpression" | "OptionalCallExpression") => {
            starts_with_object(&expression["callee"])
        }
        Some("BinaryExpression" | "LogicalExpression" | "AssignmentExpression") => {
            starts_with_object(&expression["left"])
        }
        Some("ConditionalExpression") => starts_with_object(&expression["test"]),
        Some("TaggedTemplateExpression") => starts_with_object(&expression["tag"]),
        _ => false,
    }
}

/// Whether a `new` callee contains a call, which would otherwise bind to
/// the `new`: `new (f())()` is not `new f()()`.
fn contains_call(node: &Value) -> bool {
    match node_type(node) {
        Some("CallExpression" | "OptionalCallExpression" | "OptionalMemberExpression") => true,
        Some("MemberExpression") => contains_call(&node["object"]),
        Some("TaggedTemplateExpression") => contains_call(&node["tag"]),
        _ => false,
    }
}

/// A number literal without a recorded spelling, printed the shortest way
/// that reads back to the same value.
fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e16 {
        return format!("{}", value as i64);
    }
    let text = format!("{value}");
    if value.is_finite() && value.abs() >= 1e21 {
        return format!("{value:e}").replace("e", "e+").replace("e+-", "e-");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;
    use crate::lower;

    fn printed(source: &str) -> String {
        let mut program = parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        let file = crate::babel::to_babel(program, source).unwrap();
        print(&file).unwrap().code
    }

    fn reparses(code: &str) {
        let outcome = parse(code);
        assert!(
            outcome.is_ok(),
            "printed code does not parse: {outcome:?}\n{code}"
        );
    }

    #[test]
    fn prints_statements_and_reparses() {
        let source = "import a, {b as c} from 'x';\nimport * as ns from 'y';\nexport let d = 1, e;\nexport default function f(g, {h, i: j = 2}, ...k) { if (g) { return h; } else if (j) return; else { throw new Error(); } }\nfor (let i = 0; i < 3; i++) { continue; }\nfor (const x of y) {}\nfor (const x in y) {}\nwhile (a) break;\ndo { a(); } while (b);\nswitch (x) { case 1: a(); break; default: b(); }\ntry { a(); } catch (e) { b(); } finally { c(); }\nlabel: for (;;) { break label; }\nclass K extends B { static s = 1; #p; constructor() { super(); } get x() { return 1; } static async *m() {} #q() {} static {} }\nexport { d as dd };\nexport * from 'z';\nexport * as w from 'z';\n";
        let code = printed(source);
        reparses(&code);
        assert!(code.contains("import a, { b as c } from 'x';"), "{code}");
        assert!(code.contains("export * as w from 'z';"), "{code}");
        assert!(code.contains("static async *m()"), "{code}");
    }

    #[test]
    fn brackets_follow_precedence_not_the_source() {
        let source = "const a = (1 + 2) * 3;\nconst b = 1 + 2 * 3;\nconst c = (a, b);\nconst d = (() => ({}))();\nconst e = (function () {})();\nconst f = new (g())();\nconst h = (a ?? b) || c;\nconst i = (-1) ** 2;\nconst j = a ? (b, c) : d;\nconst k = (a = 1) ? 2 : 3;\nconst l = (1).toString();\nconst m = (a?.b).c;\n({ x } = y);\nconst n = typeof (a + b);\nconst o = -(-a);\nconst p = !(a && b);\n";
        let code = printed(source);
        reparses(&code);
        assert!(code.contains("(1 + 2) * 3"), "{code}");
        assert!(code.contains("1 + 2 * 3"), "{code}");
        assert!(code.contains("(a, b)"), "{code}");
        assert!(code.contains("(() => ({}))()"), "{code}");
        assert!(code.contains("(function () {})()"), "{code}");
        assert!(code.contains("new (g())()"), "{code}");
        assert!(code.contains("(a ?? b) || c"), "{code}");
        assert!(code.contains("(-1) ** 2"), "{code}");
        assert!(code.contains("(1).toString()"), "{code}");
        assert!(code.contains("(a?.b).c"), "{code}");
        assert!(code.contains("({ x } = y);"), "{code}");
        assert!(code.contains("typeof (a + b)"), "{code}");
        assert!(code.contains("- -a"), "{code}");
    }

    #[test]
    fn prints_jsx_and_templates_verbatim() {
        let source = "const el = <Foo.Bar a=\"x\" b={1} {...rest}>text {value} <br /> &amp;</Foo.Bar>;\nconst frag = <>{items.map((i) => <li key={i}>{i}</li>)}</>;\nconst t = tag`a${b}c`;\n";
        let code = printed(source);
        reparses(&code);
        assert!(
            code.contains("<Foo.Bar a=\"x\" b={1} {...rest}>text {value} <br /> &amp;</Foo.Bar>"),
            "{code}"
        );
        assert!(code.contains("tag`a${b}c`"), "{code}");
    }

    #[test]
    fn lowered_flow_prints_as_javascript() {
        let source = "component App(a: string, ...rest: R) { const y = match (a) { 'x' => 1, _ => 2 }; return <p>{y}</p>; }\nenum E { A }\n";
        let code = printed(source);
        reparses(&code);
        assert!(code.contains("function App({ a, ...rest })"), "{code}");
        assert!(code.contains("a === 'x' ? 1 : 2"), "{code}");
        assert!(code.contains("$$ufEnumMirrored([\"A\"])"), "{code}");
    }

    #[test]
    fn mappings_point_at_the_source() {
        let source = "const a = 1;\n\nfunction f() {\n  return a;\n}\n";
        let mut program = parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        let file = crate::babel::to_babel(program, source).unwrap();
        let printed = print(&file).unwrap();
        let return_line = printed
            .code
            .lines()
            .position(|line| line.contains("return"))
            .unwrap();
        let mapping = printed
            .mappings
            .iter()
            .find(|m| m.generated_line as usize == return_line && m.generated_column == 2)
            .expect("a mapping for the return statement");
        assert_eq!(mapping.original_line, 3);
        assert_eq!(mapping.original_column, 2);
    }

    #[test]
    fn numbers_without_raw_text_print_shortest() {
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(0.5), "0.5");
        assert_eq!(format_number(1e21), "1e+21");
        assert_eq!(format_number(-3.0), "-3");
    }
}
