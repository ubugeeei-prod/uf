//! Scope analysis in Babel's terms, for the React Compiler.
//!
//! The compiler does not resolve names itself; it is handed a
//! [`ScopeInfo`] — scopes, the bindings each declares, and which binding every
//! identifier reference resolves to — built by whichever front end parsed the
//! file. Upstream that is `@babel/traverse`. This is the same analysis over
//! the Babel-shaped tree from [`crate::babel`], following Babel's rules:
//!
//! * the program, every function, every block, `for`/`for-in`/`for-of`,
//!   `switch`, `catch` and every class create a scope; a function's body
//!   block does not create a second one;
//! * `var` and function declarations hoist to the nearest function (or
//!   program) scope, `let`/`const`/`class` belong to the block they are in,
//!   parameters to their function, imports to the program;
//! * a reference is an identifier in value position — never a non-computed
//!   property key, a member name, a label, or the name of a declaration —
//!   and resolves to the nearest enclosing declaration of its name, or to
//!   nothing when it is a global.
//!
//! Nodes are addressed by the `_nodeId` the conversion assigned, with
//! `start` offsets kept beside them for the compiler's range queries.

use indexmap::IndexMap;
use react_compiler_ast::scope::{
    BindingData, BindingId, BindingKind, ImportBindingData, ImportBindingKind, ScopeData, ScopeId,
    ScopeInfo, ScopeKind,
};
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde_json::Value;

use crate::lower::{bool_field, list_field, node_type, str_field};

/// Build the scope information for a Babel `File`.
#[must_use]
pub fn analyze(file: &Value) -> ScopeInfo {
    let mut analyzer = Analyzer::default();
    let program = &file["program"];
    let root = analyzer.enter(program, ScopeKind::Program, None);
    analyzer.declare_imports(program, root);
    analyzer.hoist_statements(list_field(program, "body"), root, root);
    for statement in list_field(program, "body") {
        analyzer.visit_statement(statement, root, root);
    }
    analyzer.finish(root)
}

#[derive(Default)]
struct Analyzer {
    scopes: Vec<ScopeData>,
    bindings: Vec<BindingData>,
    node_to_scope: FxHashMap<u32, ScopeId>,
    node_to_scope_end: FxHashMap<u32, u32>,
    ref_node_id_to_binding: IndexMap<u32, BindingId, FxBuildHasher>,
    node_id_to_scope: FxHashMap<u32, ScopeId>,
}

fn node_id(node: &Value) -> Option<u32> {
    node.get("_nodeId")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
}

fn start(node: &Value) -> Option<u32> {
    node.get("start")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
}

fn end(node: &Value) -> Option<u32> {
    node.get("end")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
}

impl Analyzer {
    fn finish(self, root: ScopeId) -> ScopeInfo {
        ScopeInfo {
            scopes: self.scopes,
            bindings: self.bindings,
            node_to_scope: self.node_to_scope,
            node_to_scope_end: self.node_to_scope_end,
            reference_to_binding: IndexMap::default(),
            ref_node_id_to_binding: self.ref_node_id_to_binding,
            node_id_to_scope: self.node_id_to_scope,
            program_scope: root,
        }
    }

    fn enter(&mut self, node: &Value, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId(u32::try_from(self.scopes.len()).unwrap_or(u32::MAX));
        self.scopes.push(ScopeData {
            id,
            parent,
            kind,
            bindings: FxHashMap::default(),
        });
        if let Some(node_id) = node_id(node) {
            self.node_id_to_scope.insert(node_id, id);
        }
        if let Some(start) = start(node) {
            self.node_to_scope.insert(start, id);
            if let Some(end) = end(node) {
                self.node_to_scope_end.insert(start, end);
            }
        }
        id
    }

    fn declare(
        &mut self,
        identifier: &Value,
        scope: ScopeId,
        kind: BindingKind,
        declaration_type: &str,
        import: Option<ImportBindingData>,
    ) {
        let Some(name) = str_field(identifier, "name") else {
            return;
        };
        let existing = self.scopes[scope.0 as usize].bindings.get(name).copied();
        if let Some(existing) = existing {
            // A redeclaration — `var` twice, or a function after a `var` —
            // keeps the first binding, which is what Babel does too.
            let _ = existing;
            return;
        }
        let id = BindingId(u32::try_from(self.bindings.len()).unwrap_or(u32::MAX));
        self.bindings.push(BindingData {
            id,
            name: name.to_owned(),
            kind,
            scope,
            declaration_type: declaration_type.to_owned(),
            declaration_start: start(identifier),
            declaration_node_id: node_id(identifier),
            import,
        });
        self.scopes[scope.0 as usize]
            .bindings
            .insert(name.to_owned(), id);
    }

    fn resolve(&self, mut scope: ScopeId, name: &str) -> Option<BindingId> {
        loop {
            let data = &self.scopes[scope.0 as usize];
            if let Some(id) = data.bindings.get(name) {
                return Some(*id);
            }
            scope = data.parent?;
        }
    }

    fn reference(&mut self, identifier: &Value, scope: ScopeId) {
        let Some(name) = str_field(identifier, "name") else {
            return;
        };
        if let (Some(binding), Some(node_id)) = (self.resolve(scope, name), node_id(identifier)) {
            self.ref_node_id_to_binding.insert(node_id, binding);
        }
    }

    // ------------------------------------------------------------------
    // Declarations
    // ------------------------------------------------------------------

    fn declare_imports(&mut self, program: &Value, scope: ScopeId) {
        for statement in list_field(program, "body") {
            if node_type(statement) != Some("ImportDeclaration") {
                continue;
            }
            let source = str_field(&statement["source"], "value")
                .unwrap_or("")
                .to_owned();
            for specifier in list_field(statement, "specifiers") {
                let (kind, imported, declaration_type) = match node_type(specifier) {
                    Some("ImportDefaultSpecifier") => {
                        (ImportBindingKind::Default, None, "ImportDefaultSpecifier")
                    }
                    Some("ImportNamespaceSpecifier") => (
                        ImportBindingKind::Namespace,
                        None,
                        "ImportNamespaceSpecifier",
                    ),
                    _ => {
                        let imported = &specifier["imported"];
                        let name = str_field(imported, "name")
                            .or_else(|| str_field(imported, "value"))
                            .map(str::to_owned);
                        (ImportBindingKind::Named, name, "ImportSpecifier")
                    }
                };
                self.declare(
                    &specifier["local"],
                    scope,
                    BindingKind::Module,
                    declaration_type,
                    Some(ImportBindingData {
                        source: source.clone(),
                        kind,
                        imported,
                    }),
                );
            }
        }
    }

    /// Register what a statement list declares in `scope`, hoisting `var`
    /// into `function_scope`.
    fn hoist_statements(&mut self, statements: &[Value], scope: ScopeId, function_scope: ScopeId) {
        for statement in statements {
            self.hoist_statement(statement, scope, function_scope);
        }
    }

    fn hoist_statement(&mut self, statement: &Value, scope: ScopeId, function_scope: ScopeId) {
        match node_type(statement) {
            Some("VariableDeclaration") => self.declare_variables(statement, scope, function_scope),
            Some("FunctionDeclaration") => {
                self.declare(
                    &statement["id"],
                    scope,
                    BindingKind::Hoisted,
                    "FunctionDeclaration",
                    None,
                );
            }
            Some("ClassDeclaration") => {
                self.declare(
                    &statement["id"],
                    scope,
                    BindingKind::Let,
                    "ClassDeclaration",
                    None,
                );
            }
            Some("ExportNamedDeclaration") => {
                if let Some(declaration) = statement.get("declaration").filter(|d| !d.is_null()) {
                    self.hoist_statement(declaration, scope, function_scope);
                }
            }
            Some("ExportDefaultDeclaration") => {
                let declaration = &statement["declaration"];
                if matches!(
                    node_type(declaration),
                    Some("FunctionDeclaration" | "ClassDeclaration")
                ) && !declaration["id"].is_null()
                {
                    self.hoist_statement(declaration, scope, function_scope);
                }
            }
            // `var` declared anywhere below hoists to the function scope; walk
            // the statement containers without entering functions.
            Some("BlockStatement" | "StaticBlock") => {
                self.hoist_vars(list_field(statement, "body"), function_scope)
            }
            Some("IfStatement") => {
                self.hoist_var_statement(&statement["consequent"], function_scope);
                self.hoist_var_statement(&statement["alternate"], function_scope);
            }
            Some("ForStatement") => {
                self.hoist_var_statement(&statement["init"], function_scope);
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("ForInStatement" | "ForOfStatement") => {
                self.hoist_var_statement(&statement["left"], function_scope);
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("WhileStatement" | "DoWhileStatement" | "LabeledStatement" | "WithStatement") => {
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("TryStatement") => {
                self.hoist_var_statement(&statement["block"], function_scope);
                if !statement["handler"].is_null() {
                    self.hoist_var_statement(&statement["handler"]["body"], function_scope);
                }
                self.hoist_var_statement(&statement["finalizer"], function_scope);
            }
            Some("SwitchStatement") => {
                for case in list_field(statement, "cases") {
                    self.hoist_vars(list_field(case, "consequent"), function_scope);
                }
            }
            _ => {}
        }
    }

    /// Hoist only the `var` declarations under `statement`.
    fn hoist_var_statement(&mut self, statement: &Value, function_scope: ScopeId) {
        if statement.is_null() {
            return;
        }
        if node_type(statement) == Some("VariableDeclaration") {
            if str_field(statement, "kind") == Some("var") {
                self.declare_variables(statement, function_scope, function_scope);
            }
            return;
        }
        // Everything else: only the containers matter, and those are the
        // same cases as hoisting, minus lexical declarations.
        match node_type(statement) {
            Some("FunctionDeclaration" | "ClassDeclaration") => {}
            _ => self.hoist_statement_vars_only(statement, function_scope),
        }
    }

    fn hoist_vars(&mut self, statements: &[Value], function_scope: ScopeId) {
        for statement in statements {
            self.hoist_var_statement(statement, function_scope);
        }
    }

    fn hoist_statement_vars_only(&mut self, statement: &Value, function_scope: ScopeId) {
        match node_type(statement) {
            Some("BlockStatement" | "StaticBlock") => {
                self.hoist_vars(list_field(statement, "body"), function_scope)
            }
            Some("IfStatement") => {
                self.hoist_var_statement(&statement["consequent"], function_scope);
                self.hoist_var_statement(&statement["alternate"], function_scope);
            }
            Some("ForStatement") => {
                self.hoist_var_statement(&statement["init"], function_scope);
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("ForInStatement" | "ForOfStatement") => {
                self.hoist_var_statement(&statement["left"], function_scope);
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("WhileStatement" | "DoWhileStatement" | "LabeledStatement" | "WithStatement") => {
                self.hoist_var_statement(&statement["body"], function_scope);
            }
            Some("TryStatement") => {
                self.hoist_var_statement(&statement["block"], function_scope);
                if !statement["handler"].is_null() {
                    self.hoist_var_statement(&statement["handler"]["body"], function_scope);
                }
                self.hoist_var_statement(&statement["finalizer"], function_scope);
            }
            Some("SwitchStatement") => {
                for case in list_field(statement, "cases") {
                    self.hoist_vars(list_field(case, "consequent"), function_scope);
                }
            }
            _ => {}
        }
    }

    fn declare_variables(&mut self, declaration: &Value, scope: ScopeId, function_scope: ScopeId) {
        let (kind, target) = match str_field(declaration, "kind") {
            Some("var") => (BindingKind::Var, function_scope),
            Some("let") => (BindingKind::Let, scope),
            _ => (BindingKind::Const, scope),
        };
        for declarator in list_field(declaration, "declarations") {
            self.declare_pattern(
                &declarator["id"],
                target,
                kind.clone(),
                "VariableDeclarator",
            );
        }
    }

    /// Register every binding identifier in a pattern.
    fn declare_pattern(
        &mut self,
        pattern: &Value,
        scope: ScopeId,
        kind: BindingKind,
        declaration_type: &str,
    ) {
        match node_type(pattern) {
            Some("Identifier") => self.declare(pattern, scope, kind, declaration_type, None),
            Some("ObjectPattern") => {
                for property in list_field(pattern, "properties") {
                    match node_type(property) {
                        Some("RestElement") => {
                            self.declare_pattern(
                                &property["argument"],
                                scope,
                                kind.clone(),
                                declaration_type,
                            );
                        }
                        _ => self.declare_pattern(
                            &property["value"],
                            scope,
                            kind.clone(),
                            declaration_type,
                        ),
                    }
                }
            }
            Some("ArrayPattern") => {
                for element in list_field(pattern, "elements") {
                    if !element.is_null() {
                        self.declare_pattern(element, scope, kind.clone(), declaration_type);
                    }
                }
            }
            Some("AssignmentPattern") => {
                self.declare_pattern(&pattern["left"], scope, kind, declaration_type)
            }
            Some("RestElement") => {
                self.declare_pattern(&pattern["argument"], scope, kind, declaration_type)
            }
            _ => {}
        }
    }

    /// Visit the expressions inside a pattern: defaults and computed keys.
    fn visit_pattern_expressions(&mut self, pattern: &Value, scope: ScopeId) {
        match node_type(pattern) {
            Some("ObjectPattern") => {
                for property in list_field(pattern, "properties") {
                    match node_type(property) {
                        Some("RestElement") => {
                            self.visit_pattern_expressions(&property["argument"], scope)
                        }
                        _ => {
                            if bool_field(property, "computed") {
                                self.visit_expression(&property["key"], scope);
                            }
                            self.visit_pattern_expressions(&property["value"], scope);
                        }
                    }
                }
            }
            Some("ArrayPattern") => {
                for element in list_field(pattern, "elements") {
                    if !element.is_null() {
                        self.visit_pattern_expressions(element, scope);
                    }
                }
            }
            Some("AssignmentPattern") => {
                self.visit_pattern_expressions(&pattern["left"], scope);
                self.visit_expression(&pattern["right"], scope);
            }
            Some("RestElement") => self.visit_pattern_expressions(&pattern["argument"], scope),
            _ => {}
        }
    }

    /// A pattern in assignment position: every identifier is a reference.
    fn visit_assignment_target(&mut self, target: &Value, scope: ScopeId) {
        match node_type(target) {
            Some("Identifier") => self.reference(target, scope),
            Some("ObjectPattern") => {
                for property in list_field(target, "properties") {
                    match node_type(property) {
                        Some("RestElement") => {
                            self.visit_assignment_target(&property["argument"], scope)
                        }
                        _ => {
                            if bool_field(property, "computed") {
                                self.visit_expression(&property["key"], scope);
                            }
                            self.visit_assignment_target(&property["value"], scope);
                        }
                    }
                }
            }
            Some("ArrayPattern") => {
                for element in list_field(target, "elements") {
                    if !element.is_null() {
                        self.visit_assignment_target(element, scope);
                    }
                }
            }
            Some("AssignmentPattern") => {
                self.visit_assignment_target(&target["left"], scope);
                self.visit_expression(&target["right"], scope);
            }
            Some("RestElement") => self.visit_assignment_target(&target["argument"], scope),
            _ => self.visit_expression(target, scope),
        }
    }

    // ------------------------------------------------------------------
    // Statements
    // ------------------------------------------------------------------

    fn visit_statements(&mut self, statements: &[Value], scope: ScopeId, function_scope: ScopeId) {
        for statement in statements {
            self.visit_statement(statement, scope, function_scope);
        }
    }

    fn visit_block(&mut self, block: &Value, parent: ScopeId, function_scope: ScopeId) {
        let scope = self.enter(block, ScopeKind::Block, Some(parent));
        self.hoist_statements(list_field(block, "body"), scope, function_scope);
        self.visit_statements(list_field(block, "body"), scope, function_scope);
    }

    fn visit_statement(&mut self, statement: &Value, scope: ScopeId, function_scope: ScopeId) {
        match node_type(statement) {
            Some("ExpressionStatement") => self.visit_expression(&statement["expression"], scope),
            Some("VariableDeclaration") => {
                for declarator in list_field(statement, "declarations") {
                    self.visit_pattern_expressions(&declarator["id"], scope);
                    self.visit_expression(&declarator["init"], scope);
                }
            }
            Some("FunctionDeclaration") => self.visit_function(statement, scope),
            Some("ClassDeclaration" | "ClassExpression") => self.visit_class(statement, scope),
            Some("ReturnStatement" | "ThrowStatement") => {
                self.visit_expression(&statement["argument"], scope)
            }
            Some("BlockStatement") => self.visit_block(statement, scope, function_scope),
            Some("StaticBlock") => {
                let inner = self.enter(statement, ScopeKind::Block, Some(scope));
                self.hoist_statements(list_field(statement, "body"), inner, inner);
                self.visit_statements(list_field(statement, "body"), inner, inner);
            }
            Some("IfStatement") => {
                self.visit_expression(&statement["test"], scope);
                self.visit_statement(&statement["consequent"], scope, function_scope);
                self.visit_statement(&statement["alternate"], scope, function_scope);
            }
            Some("ForStatement") => {
                let inner = self.enter(statement, ScopeKind::For, Some(scope));
                let init = &statement["init"];
                if node_type(init) == Some("VariableDeclaration") {
                    if str_field(init, "kind") != Some("var") {
                        self.declare_variables(init, inner, function_scope);
                    }
                    self.visit_statement(init, inner, function_scope);
                } else {
                    self.visit_expression(init, inner);
                }
                self.visit_expression(&statement["test"], inner);
                self.visit_expression(&statement["update"], inner);
                self.visit_statement(&statement["body"], inner, function_scope);
            }
            Some("ForInStatement" | "ForOfStatement") => {
                let inner = self.enter(statement, ScopeKind::For, Some(scope));
                let left = &statement["left"];
                if node_type(left) == Some("VariableDeclaration") {
                    if str_field(left, "kind") != Some("var") {
                        self.declare_variables(left, inner, function_scope);
                    }
                    self.visit_statement(left, inner, function_scope);
                } else {
                    self.visit_assignment_target(left, inner);
                }
                self.visit_expression(&statement["right"], inner);
                self.visit_statement(&statement["body"], inner, function_scope);
            }
            Some("WhileStatement") => {
                self.visit_expression(&statement["test"], scope);
                self.visit_statement(&statement["body"], scope, function_scope);
            }
            Some("DoWhileStatement") => {
                self.visit_statement(&statement["body"], scope, function_scope);
                self.visit_expression(&statement["test"], scope);
            }
            Some("LabeledStatement") => {
                self.visit_statement(&statement["body"], scope, function_scope)
            }
            Some("WithStatement") => {
                self.visit_expression(&statement["object"], scope);
                self.visit_statement(&statement["body"], scope, function_scope);
            }
            Some("SwitchStatement") => {
                self.visit_expression(&statement["discriminant"], scope);
                let inner = self.enter(statement, ScopeKind::Switch, Some(scope));
                for case in list_field(statement, "cases") {
                    self.hoist_statements(list_field(case, "consequent"), inner, function_scope);
                }
                for case in list_field(statement, "cases") {
                    self.visit_expression(&case["test"], inner);
                    self.visit_statements(list_field(case, "consequent"), inner, function_scope);
                }
            }
            Some("TryStatement") => {
                self.visit_statement(&statement["block"], scope, function_scope);
                let handler = &statement["handler"];
                if !handler.is_null() {
                    let inner = self.enter(handler, ScopeKind::Catch, Some(scope));
                    if !handler["param"].is_null() {
                        self.declare_pattern(
                            &handler["param"],
                            inner,
                            BindingKind::Let,
                            "CatchClause",
                        );
                        self.visit_pattern_expressions(&handler["param"], inner);
                    }
                    self.visit_statement(&handler["body"], inner, function_scope);
                }
                self.visit_statement(&statement["finalizer"], scope, function_scope);
            }
            Some("ExportNamedDeclaration") => {
                let declaration = &statement["declaration"];
                if !declaration.is_null() {
                    self.visit_statement(declaration, scope, function_scope);
                }
                if statement["source"].is_null() {
                    for specifier in list_field(statement, "specifiers") {
                        if node_type(specifier) == Some("ExportSpecifier") {
                            self.reference(&specifier["local"], scope);
                        }
                    }
                }
            }
            Some("ExportDefaultDeclaration") => {
                let declaration = &statement["declaration"];
                match node_type(declaration) {
                    Some("FunctionDeclaration") => self.visit_function(declaration, scope),
                    Some("ClassDeclaration") => self.visit_class(declaration, scope),
                    _ => self.visit_expression(declaration, scope),
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------
    // Functions and classes
    // ------------------------------------------------------------------

    fn visit_function(&mut self, function: &Value, parent: ScopeId) {
        let scope = self.enter(function, ScopeKind::Function, Some(parent));
        if node_type(function) == Some("FunctionExpression") && !function["id"].is_null() {
            self.declare(
                &function["id"],
                scope,
                BindingKind::Local,
                "FunctionExpression",
                None,
            );
        }
        for param in list_field(function, "params") {
            let declaration_type = node_type(param).unwrap_or("Identifier").to_owned();
            self.declare_pattern(param, scope, BindingKind::Param, &declaration_type);
        }
        let body = &function["body"];
        if node_type(body) == Some("BlockStatement") {
            self.hoist_statements(list_field(body, "body"), scope, scope);
        }
        for param in list_field(function, "params") {
            self.visit_pattern_expressions(param, scope);
        }
        if node_type(body) == Some("BlockStatement") {
            self.visit_statements(list_field(body, "body"), scope, scope);
        } else {
            self.visit_expression(body, scope);
        }
    }

    fn visit_class(&mut self, class: &Value, parent: ScopeId) {
        self.visit_expression(&class["superClass"], parent);
        let scope = self.enter(class, ScopeKind::Class, Some(parent));
        if node_type(class) == Some("ClassExpression") && !class["id"].is_null() {
            self.declare(
                &class["id"],
                scope,
                BindingKind::Local,
                "ClassExpression",
                None,
            );
        }
        for member in list_field(&class["body"], "body") {
            match node_type(member) {
                Some("ClassMethod" | "ClassPrivateMethod") => {
                    if bool_field(member, "computed") {
                        self.visit_expression(&member["key"], scope);
                    }
                    self.visit_function(member, scope);
                }
                Some("ClassProperty" | "ClassPrivateProperty") => {
                    if bool_field(member, "computed") {
                        self.visit_expression(&member["key"], scope);
                    }
                    self.visit_expression(&member["value"], scope);
                }
                Some("StaticBlock") => self.visit_statement(member, scope, scope),
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------
    // Expressions
    // ------------------------------------------------------------------

    fn visit_expressions(&mut self, expressions: &[Value], scope: ScopeId) {
        for expression in expressions {
            self.visit_expression(expression, scope);
        }
    }

    fn visit_expression(&mut self, expression: &Value, scope: ScopeId) {
        if expression.is_null() {
            return;
        }
        match node_type(expression) {
            Some("Identifier") => self.reference(expression, scope),
            Some("ArrowFunctionExpression" | "FunctionExpression") => {
                self.visit_function(expression, scope)
            }
            Some("ClassExpression") => self.visit_class(expression, scope),
            Some("MemberExpression" | "OptionalMemberExpression") => {
                self.visit_expression(&expression["object"], scope);
                if bool_field(expression, "computed") {
                    self.visit_expression(&expression["property"], scope);
                }
            }
            Some("CallExpression" | "OptionalCallExpression" | "NewExpression") => {
                self.visit_expression(&expression["callee"], scope);
                self.visit_expressions(list_field(expression, "arguments"), scope);
            }
            Some("AssignmentExpression") => {
                self.visit_assignment_target(&expression["left"], scope);
                self.visit_expression(&expression["right"], scope);
            }
            Some("UpdateExpression") => {
                self.visit_assignment_target(&expression["argument"], scope)
            }
            Some("ObjectExpression") => {
                for property in list_field(expression, "properties") {
                    match node_type(property) {
                        Some("SpreadElement") => {
                            self.visit_expression(&property["argument"], scope)
                        }
                        Some("ObjectMethod") => {
                            if bool_field(property, "computed") {
                                self.visit_expression(&property["key"], scope);
                            }
                            self.visit_function(property, scope);
                        }
                        _ => {
                            if bool_field(property, "computed") {
                                self.visit_expression(&property["key"], scope);
                            }
                            self.visit_expression(&property["value"], scope);
                        }
                    }
                }
            }
            Some("ArrayExpression") => {
                for element in list_field(expression, "elements") {
                    self.visit_expression(element, scope);
                }
            }
            Some("TemplateLiteral") => {
                self.visit_expressions(list_field(expression, "expressions"), scope)
            }
            Some("TaggedTemplateExpression") => {
                self.visit_expression(&expression["tag"], scope);
                self.visit_expression(&expression["quasi"], scope);
            }
            Some("JSXElement") => {
                self.visit_jsx_name(&expression["openingElement"]["name"], scope);
                for attribute in list_field(&expression["openingElement"], "attributes") {
                    match node_type(attribute) {
                        Some("JSXSpreadAttribute") => {
                            self.visit_expression(&attribute["argument"], scope)
                        }
                        _ => self.visit_expression(&attribute["value"], scope),
                    }
                }
                self.visit_expressions(list_field(expression, "children"), scope);
            }
            Some("JSXFragment") => {
                self.visit_expressions(list_field(expression, "children"), scope)
            }
            Some("JSXExpressionContainer" | "JSXSpreadChild") => {
                self.visit_expression(&expression["expression"], scope);
            }
            Some("SpreadElement" | "AwaitExpression" | "YieldExpression" | "UnaryExpression") => {
                self.visit_expression(&expression["argument"], scope);
            }
            Some("ParenthesizedExpression") => {
                self.visit_expression(&expression["expression"], scope)
            }
            Some("BinaryExpression" | "LogicalExpression") => {
                self.visit_expression(&expression["left"], scope);
                self.visit_expression(&expression["right"], scope);
            }
            Some("ConditionalExpression") => {
                self.visit_expression(&expression["test"], scope);
                self.visit_expression(&expression["consequent"], scope);
                self.visit_expression(&expression["alternate"], scope);
            }
            Some("SequenceExpression") => {
                self.visit_expressions(list_field(expression, "expressions"), scope)
            }
            Some("AssignmentPattern") => {
                self.visit_assignment_target(&expression["left"], scope);
                self.visit_expression(&expression["right"], scope);
            }
            Some("ObjectPattern" | "ArrayPattern" | "RestElement") => {
                self.visit_assignment_target(expression, scope)
            }
            _ => {}
        }
    }

    /// `<Foo>` and `<Foo.Bar>` reference `Foo`; `<div>` and `<foo:bar>` do not.
    fn visit_jsx_name(&mut self, name: &Value, scope: ScopeId) {
        match node_type(name) {
            Some("JSXIdentifier") => {
                if str_field(name, "name")
                    .is_some_and(|text| text.chars().next().is_some_and(char::is_uppercase))
                {
                    self.reference(name, scope);
                }
            }
            Some("JSXMemberExpression") => {
                let mut object = &name["object"];
                while node_type(object) == Some("JSXMemberExpression") {
                    object = &object["object"];
                }
                self.reference(object, scope);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estree::parse;
    use crate::lower;

    fn analyzed(source: &str) -> (Value, ScopeInfo) {
        let mut program = parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        let file = crate::babel::to_babel(program, source).unwrap();
        let info = analyze(&file);
        (file, info)
    }

    fn binding_names(info: &ScopeInfo, scope: ScopeId) -> Vec<String> {
        let mut names: Vec<String> = info.scopes[scope.0 as usize]
            .bindings
            .keys()
            .cloned()
            .collect();
        names.sort();
        names
    }

    #[test]
    fn imports_and_top_level_declarations_live_in_the_program_scope() {
        let (_, info) = analyzed(
            "import {a} from 'a';\nimport b from 'b';\nconst c = 1;\nfunction d() {}\nclass E {}\n",
        );
        assert_eq!(binding_names(&info, ScopeId(0)), ["E", "a", "b", "c", "d"]);
        let a = &info.bindings[info.scopes[0].bindings["a"].0 as usize];
        assert!(matches!(a.kind, BindingKind::Module));
        assert_eq!(a.import.as_ref().unwrap().source, "a");
        assert_eq!(a.import.as_ref().unwrap().imported.as_deref(), Some("a"));
    }

    #[test]
    fn var_hoists_to_the_function_and_let_stays_in_its_block() {
        let (_, info) = analyzed("function f() { if (x) { var v = 1; let l = 2; } }\n");
        let function = info
            .scopes
            .iter()
            .find(|scope| matches!(scope.kind, ScopeKind::Function))
            .unwrap();
        assert_eq!(binding_names(&info, function.id), ["v"]);
        let block = info
            .scopes
            .iter()
            .find(|scope| matches!(scope.kind, ScopeKind::Block))
            .unwrap();
        assert_eq!(binding_names(&info, block.id), ["l"]);
    }

    #[test]
    fn references_resolve_to_the_nearest_declaration() {
        let (file, info) = analyzed("const n = 1;\nfunction f(n) { return n + m; }\n");
        let function = &file["program"]["body"][1];
        let returned = &function["body"]["body"][0]["argument"];
        let inner_n = node_id(&returned["left"]).unwrap();
        let m = node_id(&returned["right"]).unwrap();
        let binding = info.ref_node_id_to_binding[&inner_n];
        assert!(matches!(
            info.bindings[binding.0 as usize].kind,
            BindingKind::Param
        ));
        assert!(
            !info.ref_node_id_to_binding.contains_key(&m),
            "a global resolves to nothing"
        );
    }

    #[test]
    fn property_keys_and_member_names_are_not_references() {
        let (file, info) = analyzed("const a = 1;\nconst o = { a: a.a };\n");
        let property = &file["program"]["body"][1]["declarations"][0]["init"]["properties"][0];
        let key = node_id(&property["key"]).unwrap();
        let member_name = node_id(&property["value"]["property"]).unwrap();
        let object = node_id(&property["value"]["object"]).unwrap();
        assert!(!info.ref_node_id_to_binding.contains_key(&key));
        assert!(!info.ref_node_id_to_binding.contains_key(&member_name));
        assert!(info.ref_node_id_to_binding.contains_key(&object));
    }

    #[test]
    fn a_component_s_scope_is_keyed_by_its_node() {
        let (file, info) = analyzed(
            "component App(title: string) { return <Title>{title}</Title>; }\nconst Title = null;\n",
        );
        let function = &file["program"]["body"][0];
        let scope = info.resolve_scope_for_node(node_id(function)).unwrap();
        assert!(matches!(
            info.scopes[scope.0 as usize].kind,
            ScopeKind::Function
        ));
        assert_eq!(binding_names(&info, scope), ["title"]);
        let jsx = &function["body"]["body"][0]["argument"]["openingElement"]["name"];
        assert!(
            info.ref_node_id_to_binding
                .contains_key(&node_id(jsx).unwrap())
        );
    }
}
