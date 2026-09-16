//! The JSX rules `eslint-plugin-react` users know, answered from the tree.
//!
//! * `react/jsx-key` — an element built in an array literal, or returned by a
//!   `map`, `flatMap` or `Array.from` callback, carries a `key`.
//! * `react/no-array-index-key` — and that `key` is not the index the callback
//!   was handed.
//! * `react/jsx-no-duplicate-props` — no prop is written twice on one element.
//! * `react/no-children-prop` — `children` arrive between the tags.
//! * `react/void-dom-elements-no-children` — `<img>`, `<br>` and the other
//!   void elements hold nothing.
//! * `react/jsx-no-comment-textnodes` — `//` and `/*` between tags are text.
//!
//! Every one of them asks how nodes relate: which callback an element is
//! returned from, which parameter a key names, what an element holds. So, like
//! [`super::tree`], this reads the official parser's tree through the port's
//! own visitor, and it is handed the same parse by [`super::module_tree`]. It
//! is a walk of its own rather than more of `tree`'s because what it carries
//! differs — `tree` carries the host elements above a node, this carries the
//! parameters in scope — and a walk over a module that is already parsed costs
//! a small fraction of the parse.
//!
//! # What each rule leaves alone
//!
//! A linter is judged by what it does not say, and each rule is silent where
//! the answer is not written in the file:
//!
//! * An element carrying `{...spread}` may be carrying the `key` or the
//!   children; a spread is not read. A spread *object literal* is, because its
//!   keys are written right there.
//! * `React.Children.map` keys what its callback returns by itself, so
//!   `react/jsx-key` asks nothing of it.
//! * `Array.from({ length: n }, (_, i) => …)` builds a list that never
//!   reorders, which is exactly when an index is a correct key, so
//!   `react/no-array-index-key` does not follow `Array.from`.
//! * A key that holds the item as well as its index, `${index}:${word}`,
//!   changes when the item does, so React remounts the element rather than
//!   handing it another item's state. An empty host element that is not a
//!   form control, a media or embedded element, a disclosure or a canvas keeps
//!   no state for a key to carry. `react/no-array-index-key` reports neither.
//! * Text inside `<code>` and `<pre>` is meant to be read, slashes and all.
//!
//! * The branches of one conditional are one slot of an array, and only one of
//!   them ever fills it, so the `key` they share is one key rather than a
//!   duplicate.
//!
//! Two shapes the plugin passes over are reported, because each is the same
//! defect as the rule's own: a `<>` fragment in a list cannot take a key at
//! all, and two elements with one literal `key` in one array are one element
//! to React.

use std::borrow::Cow;

use uf_config::UniflowedConfig;
use uf_flow::ast::expression::{ExpressionInner, ExpressionOrSpread};
use uf_flow::ast::jsx;
use uf_flow::ast_visitor::{self, AstVisitor};
use uf_flow::{Loc, ast};
use uf_profiler::profile_span;

use super::tree::{attribute, has_code_jsx_marker, has_spread, host_name};
use crate::scan::FileScan;
use crate::{Diagnostic, Severity, push_at, severity};

type Expression = ast::expression::Expression<Loc, Loc>;
type Function = ast::function::Function<Loc, Loc>;
type Statement = ast::statement::Statement<Loc, Loc>;

/// `react/jsx-key`.
const JSX_KEY: &str = "react/jsx-key";

/// `react/no-array-index-key`.
const ARRAY_INDEX_KEY: &str = "react/no-array-index-key";

/// `react/jsx-no-duplicate-props`.
const DUPLICATE_PROPS: &str = "react/jsx-no-duplicate-props";

/// `react/no-children-prop`.
const CHILDREN_PROP: &str = "react/no-children-prop";

/// `react/void-dom-elements-no-children`.
const VOID_CHILDREN: &str = "react/void-dom-elements-no-children";

/// `react/jsx-no-comment-textnodes`.
const COMMENT_TEXT: &str = "react/jsx-no-comment-textnodes";

/// Host elements that may hold neither children nor `dangerouslySetInnerHTML`.
///
/// React's own list, which is the list it throws for.
static VOID_ELEMENTS: phf::Set<&'static str> = phf::phf_set! {
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "menuitem",
    "meta", "param", "source", "track", "wbr",
};

/// Host elements whose text is shown as written, so a `//` in it is meant.
const LITERAL_TEXT: [&str; 2] = ["code", "pre"];

/// What this runner wants out of a parse, or [`None`] when it wants none.
///
/// Split from the walk so that one parse serves every runner that needs the
/// module's tree — see [`super::module_tree`], which owns it.
pub(super) struct JsxWork {
    levels: Levels,
}

/// Whether these rules want this module read at all.
pub(super) fn wanted(scan: &FileScan<'_>, config: &UniflowedConfig) -> Option<JsxWork> {
    let levels = Levels::for_config(config);
    if levels.all_off() {
        return None;
    }
    if !super::flow_syntax::is_flow_syntax_target(&scan.file.path) {
        return None;
    }
    let wants = has_code_jsx_marker(scan)
        || (levels.comment_text.is_some() && has_comment_after_a_tag(scan))
        || (levels.reads_element_calls() && has_code_element_call(scan));
    wants.then_some(JsxWork { levels })
}

/// Whether a line's code ends at a tag and what the scanner took for a
/// comment comes straight after it.
///
/// `<div>// empty div</div>` is the shape `react/jsx-no-comment-textnodes`
/// exists for, and to the line scanner it is `<div>` followed by a comment —
/// so the `</div>` that [`has_code_jsx_marker`] looks for is inside the part
/// it skipped, and a component whose only JSX is that line would never be
/// read.
fn has_comment_after_a_tag(scan: &FileScan<'_>) -> bool {
    let source = &scan.file.source;
    if !source.contains("//") && !source.contains("/*") {
        return false;
    }
    scan.lines.iter().any(|line| {
        let code = line.code();
        let Some(rest) = line.text.get(line.code_offset() + code.len()..) else {
            return false;
        };
        let rest = rest.trim_start();
        code.trim_end().ends_with('>') && (rest.starts_with("//") || rest.starts_with("/*"))
    })
}

/// Whether the code calls `createElement` or `cloneElement`, which three of
/// these rules read as well as JSX.
fn has_code_element_call(scan: &FileScan<'_>) -> bool {
    const CALLS: [&str; 2] = ["createElement", "cloneElement"];

    let source = &scan.file.source;
    if !CALLS.iter().any(|call| source.contains(call)) {
        return false;
    }
    scan.lines.iter().any(|line| {
        let code = line.code();
        CALLS
            .iter()
            .any(|call| code.match_indices(call).any(|(at, _)| !line.in_string(at)))
    })
}

/// Walk a tree somebody else parsed.
///
/// # Call this on the thread that built `parsed`
///
/// The walk recurses once per level of the tree, so it needs the stack
/// `uf_flow::PARSE_STACK_BYTES` names for the same reason the parse does.
/// [`super::module_tree`] is the caller, and it is on one.
pub(super) fn walk(parsed: &uf_flow::Parsed, work: &JsxWork) -> Vec<Finding> {
    profile_span!("run_react_jsx_rules");
    let levels = &work.levels;
    let mut walk = Walk {
        jsx_key: levels.jsx_key.is_some(),
        index_key: levels.index_key.is_some(),
        duplicate_props: levels.duplicate_props.is_some(),
        children_prop: levels.children_prop.is_some(),
        void_children: levels.void_children.is_some(),
        comment_text: levels.comment_text.is_some(),
        params: Vec::new(),
        pending_iteration: None,
        literal: 0,
        found: Vec::new(),
    };
    // Nothing in the walk fails; the `Result` in the visitor's signature is
    // for visitors that stop early, and this one never does.
    let _ = walk.program(&parsed.program);
    let mut found = walk.found;
    found.sort_by_key(|finding| (finding.line, finding.column, finding.rule));
    found.dedup_by(|a, b| a.line == b.line && a.column == b.column && a.rule == b.rule);
    found
}

/// Turn what the walk found into diagnostics.
pub(super) fn report(
    scan: &FileScan<'_>,
    work: &JsxWork,
    found: Vec<Finding>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for finding in found {
        let Some(severity) = work.levels.of(finding.rule) else {
            continue;
        };
        let Some(index) = usize::try_from(finding.line)
            .ok()
            .and_then(|line| line.checked_sub(1))
        else {
            continue;
        };
        let Some(line) = scan.lines.get(index) else {
            continue;
        };
        // The port counts columns in bytes, which is what `push_at` wants, but
        // a position past the end of the line would resolve into the next one.
        let column = usize::try_from(finding.column)
            .unwrap_or(0)
            .min(line.text.len());
        push_at(
            diagnostics,
            scan,
            finding.rule,
            severity,
            index,
            column,
            finding.message,
        );
    }
}

/// Configured severity for each rule this runner owns.
struct Levels {
    jsx_key: Option<Severity>,
    index_key: Option<Severity>,
    duplicate_props: Option<Severity>,
    children_prop: Option<Severity>,
    void_children: Option<Severity>,
    comment_text: Option<Severity>,
}

impl Levels {
    fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            jsx_key: severity(config, JSX_KEY),
            index_key: severity(config, ARRAY_INDEX_KEY),
            duplicate_props: severity(config, DUPLICATE_PROPS),
            children_prop: severity(config, CHILDREN_PROP),
            void_children: severity(config, VOID_CHILDREN),
            comment_text: severity(config, COMMENT_TEXT),
        }
    }

    fn all_off(&self) -> bool {
        self.jsx_key.is_none()
            && self.index_key.is_none()
            && self.duplicate_props.is_none()
            && self.children_prop.is_none()
            && self.void_children.is_none()
            && self.comment_text.is_none()
    }

    /// Whether a rule that also reads `createElement` and `cloneElement` is on.
    fn reads_element_calls(&self) -> bool {
        self.index_key.is_some() || self.children_prop.is_some() || self.void_children.is_some()
    }

    fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            JSX_KEY => self.jsx_key,
            ARRAY_INDEX_KEY => self.index_key,
            DUPLICATE_PROPS => self.duplicate_props,
            CHILDREN_PROP => self.children_prop,
            VOID_CHILDREN => self.void_children,
            COMMENT_TEXT => self.comment_text,
            _ => None,
        }
    }
}

/// One finding, positioned the way the port positions nodes.
pub(super) struct Finding {
    rule: &'static str,
    /// 1-based line.
    line: i32,
    /// 0-based byte column within that line.
    column: i32,
    message: String,
}

/// Where an element that needs a key was built.
#[derive(Clone, Copy)]
enum List {
    /// An element of an array literal.
    Array,
    /// Returned from the callback of `map`, `flatMap` or `Array.from`.
    Callback(&'static str),
}

/// The two element factories React exports.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ElementApi {
    Create,
    Clone,
}

/// The walk: one pass over the tree, carrying what the rules need to know.
struct Walk<'ast> {
    jsx_key: bool,
    index_key: bool,
    duplicate_props: bool,
    children_prop: bool,
    void_children: bool,
    comment_text: bool,
    /// Parameters of the functions the walk is inside, innermost last, each
    /// with what it is to the iteration that calls its function.
    params: Vec<(&'ast str, Param)>,
    /// The iteration whose callback is the function about to be entered.
    pending_iteration: Option<Iteration>,
    /// How many `<code>` and `<pre>` elements enclose the node being visited.
    literal: u32,
    found: Vec<Finding>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Walk<'ast> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    /// `react/jsx-key` for the elements of an array literal.
    fn array(
        &mut self,
        loc: &'ast Loc,
        array: &'ast ast::expression::Array<Loc, Loc>,
    ) -> Result<(), ()> {
        if self.jsx_key {
            self.check_array_keys(array);
        }
        ast_visitor::array_default(self, loc, array)
    }

    /// The list callbacks and the element factories, and then a walk that
    /// marks the index parameter of the callback an iteration method is given.
    fn call(
        &mut self,
        loc: &'ast Loc,
        call: &'ast ast::expression::Call<Loc, Loc>,
    ) -> Result<(), ()> {
        if self.jsx_key {
            self.check_callback_keys(call);
        }
        if self.index_key || self.children_prop || self.void_children {
            self.check_element_call(loc, call);
        }

        let iteration = if self.index_key {
            iteration_of(&call.callee)
        } else {
            None
        };
        self.expression(&call.callee)?;
        for (position, argument) in call.arguments.arguments.iter().enumerate() {
            match argument {
                ExpressionOrSpread::Expression(argument) => {
                    // Set only for a function written right there, so that it
                    // is that function and no other that consumes it.
                    if let Some(iteration) = iteration
                        && iteration.callback == position
                        && as_function(argument).is_some()
                    {
                        self.pending_iteration = Some(iteration);
                    }
                    let walked = self.expression(argument);
                    self.pending_iteration = None;
                    walked?;
                }
                ExpressionOrSpread::Spread(spread) => self.expression(&spread.argument)?,
            }
        }
        Ok(())
    }

    /// Every function, which is where parameters come into scope.
    fn function_(&mut self, loc: &'ast Loc, function: &'ast Function) -> Result<(), ()> {
        let iteration = self.pending_iteration.take();
        let outer = self.params.len();
        for (position, param) in function.params.params.iter().enumerate() {
            let ast::function::Param::RegularParam { argument, .. } = param else {
                continue;
            };
            if let ast::pattern::Pattern::Identifier { inner, .. } = argument {
                let role = match iteration {
                    Some(iteration) if iteration.index == position => Param::Index,
                    Some(iteration) if iteration.item == position => Param::Item,
                    _ => Param::Other,
                };
                self.params.push((&inner.name.name, role));
            }
        }
        let walked = ast_visitor::function_default(self, loc, function);
        self.params.truncate(outer);
        walked
    }

    /// A JSX element, walked by this module rather than by the default so an
    /// attribute's value is not counted as being inside `<code>`.
    fn jsx_element(
        &mut self,
        _loc: &'ast Loc,
        element: &'ast jsx::Element<Loc, Loc>,
    ) -> Result<(), ()> {
        let opening = &element.opening_element;
        let tag = host_name(&opening.name);
        if self.duplicate_props {
            self.check_duplicate_props(opening);
        }
        if self.children_prop {
            self.check_children_prop(element);
        }
        if self.index_key {
            self.check_index_key(element);
        }
        if self.void_children
            && let Some(name) = tag
        {
            self.check_void_children(name, element);
        }
        let literal = tag.is_some_and(|name| LITERAL_TEXT.contains(&name));
        if self.comment_text && self.literal == 0 && !literal {
            self.check_comment_text(&element.children.1);
        }

        // An attribute value is an expression handed to this element, not
        // text rendered inside whatever encloses it.
        let outer = std::mem::take(&mut self.literal);
        for attribute in opening.attributes.iter() {
            match attribute {
                jsx::OpeningAttribute::Attribute(attribute) => {
                    if let Some(jsx::attribute::Value::ExpressionContainer((_, container))) =
                        &attribute.value
                    {
                        self.container(container)?;
                    }
                }
                jsx::OpeningAttribute::SpreadAttribute(spread) => {
                    self.expression(&spread.argument)?;
                }
            }
        }
        self.literal = outer + u32::from(literal);
        let walked = self.children(&element.children.1);
        self.literal = outer;
        walked
    }

    fn jsx_fragment(
        &mut self,
        _loc: &'ast Loc,
        fragment: &'ast jsx::Fragment<Loc, Loc>,
    ) -> Result<(), ()> {
        if self.comment_text && self.literal == 0 {
            self.check_comment_text(&fragment.frag_children.1);
        }
        self.children(&fragment.frag_children.1)
    }
}

impl<'ast> Walk<'ast> {
    fn children(&mut self, children: &'ast [jsx::Child<Loc, Loc>]) -> Result<(), ()> {
        for child in children {
            match child {
                jsx::Child::Element { loc, inner } => self.jsx_element(loc, inner)?,
                jsx::Child::Fragment { loc, inner } => self.jsx_fragment(loc, inner)?,
                jsx::Child::ExpressionContainer { inner, .. } => self.container(inner)?,
                jsx::Child::SpreadChild { inner, .. } => self.expression(&inner.expression)?,
                jsx::Child::Text { .. } => {}
            }
        }
        Ok(())
    }

    fn container(&mut self, container: &'ast jsx::ExpressionContainer<Loc, Loc>) -> Result<(), ()> {
        match &container.expression {
            jsx::expression_container::Expression::Expression(expression) => {
                self.expression(expression)
            }
            jsx::expression_container::Expression::EmptyExpression => Ok(()),
        }
    }

    fn report(&mut self, loc: &Loc, rule: &'static str, message: String) {
        self.found.push(Finding {
            rule,
            line: loc.start.line,
            column: loc.start.column,
            message,
        });
    }

    // --- react/jsx-key ------------------------------------------------------

    /// Every element an array literal holds, and every literal `key` it holds
    /// twice.
    ///
    /// A key is compared only with the keys of the slots *before* its own. One
    /// slot builds more than one element when a conditional writes it, and
    /// those branches exclude each other: in
    /// `[flag ? <A key="x" /> : <B key="x" />]` one element reaches the array,
    /// so the shared `"x"` is the same key on the same slot rather than two
    /// elements React cannot tell apart.
    fn check_array_keys(&mut self, array: &'ast ast::expression::Array<Loc, Loc>) {
        let mut keys: Vec<&'ast str> = Vec::new();
        // Held across slots so the keys of one slot are added only once it is
        // finished, and its own branches are never compared with each other.
        let mut slot: Vec<&'ast str> = Vec::new();
        for element in array.elements.iter() {
            let ast::expression::ArrayElement::Expression(expression) = element else {
                continue;
            };
            let mut built = Vec::new();
            rendered_elements(expression, &mut built);
            for node in built {
                self.require_key(node, List::Array);
                let Some((loc, key)) = literal_key(node) else {
                    continue;
                };
                if keys.contains(&key) {
                    self.report(
                        loc,
                        JSX_KEY,
                        format!(
                            "`key=\"{key}\"` is used twice in this array, and React tells elements \
                             apart by key, so it may drop one of them or give it the other's state; \
                             give each element a key of its own"
                        ),
                    );
                } else if !slot.contains(&key) {
                    slot.push(key);
                }
            }
            keys.append(&mut slot);
        }
    }

    /// The elements a `map`, `flatMap` or `Array.from` callback returns.
    fn check_callback_keys(&mut self, call: &'ast ast::expression::Call<Loc, Loc>) {
        let Some((method, callback)) = list_callback(call) else {
            return;
        };
        let Some(function) = as_function(callback) else {
            return;
        };
        let mut built = Vec::new();
        returned_elements(function, &mut built);
        for node in built {
            self.require_key(node, List::Callback(method));
        }
    }

    /// Report `node` when it is an element built into a list without a key.
    fn require_key(&mut self, node: &'ast Expression, list: List) {
        match &**node {
            ExpressionInner::JSXElement { inner, .. } => {
                let opening = &inner.opening_element;
                if attribute(opening, "key").is_some() {
                    return;
                }
                let name = element_name(&opening.name);
                if let Some(loc) = key_in_spread_object(opening) {
                    self.report(
                        loc,
                        JSX_KEY,
                        format!(
                            "this `key` reaches `<{name}>` inside a spread object, which React warns \
                             about because a key has to be written on the element; write `key={{…}}` \
                             on `<{name}>` itself"
                        ),
                    );
                    return;
                }
                // The spread may be carrying the key, and it is not read.
                if has_spread(opening) {
                    return;
                }
                let message = match list {
                    List::Array => format!(
                        "`<{name}>` sits in an array with no `key`, so React matches it to the next \
                         render by position; give it a `key` that says which item it is"
                    ),
                    List::Callback(method) => format!(
                        "`<{name}>` is returned from `{method}` with no `key`, so React matches the \
                         items by position and hands one item's state to another when the list is \
                         reordered or filtered; give it `key={{…}}` with the item's id"
                    ),
                };
                self.report(&opening.loc, JSX_KEY, message);
            }
            ExpressionInner::JSXFragment { loc, .. } => {
                let place = match list {
                    List::Array => String::from("in an array"),
                    List::Callback(method) => format!("returned from `{method}`"),
                };
                self.report(
                    loc,
                    JSX_KEY,
                    format!(
                        "a `<>` fragment {place} cannot take a `key`, so React matches it by \
                         position; write `<React.Fragment key={{…}}>` instead"
                    ),
                );
            }
            _ => {}
        }
    }

    // --- react/no-array-index-key -------------------------------------------

    /// A `key` attribute built from the index of an enclosing iteration.
    ///
    /// Not on an empty host element that keeps no state of its own — a blank
    /// `<td />` in a calendar row, keyed by its column — because a key only
    /// matters for what it carries across renders, and that element carries
    /// nothing.
    fn check_index_key(&mut self, element: &'ast jsx::Element<Loc, Loc>) {
        let opening = &element.opening_element;
        let Some(key) = attribute(opening, "key") else {
            return;
        };
        let Some(jsx::attribute::Value::ExpressionContainer((_, container))) = &key.value else {
            return;
        };
        let jsx::expression_container::Expression::Expression(value) = &container.expression else {
            return;
        };
        let carries_nothing = host_name(&opening.name).is_some_and(holds_no_state)
            && !has_spread(opening)
            && attribute(opening, "children").is_none()
            && attribute(opening, "dangerouslySetInnerHTML").is_none()
            && !element.children.1.iter().any(renders_something);
        if carries_nothing {
            return;
        }
        if let Some(index) = self.position_key(value) {
            self.report(&key.loc, ARRAY_INDEX_KEY, index_message(index));
        }
    }

    /// The index a key is built from, when the key says nothing but where the
    /// item stands.
    ///
    /// A key that also holds the item itself — `${index}:${word}` — says more:
    /// it changes when the item at that position does, so React remounts the
    /// element instead of handing it the state of the item that stood there
    /// before. A property of the item is not enough: `${item.kind}-${index}`
    /// stays the same when two items of one kind swap places.
    fn position_key(&self, key: &'ast Expression) -> Option<&'ast str> {
        let index = self.index_in(key)?;
        (!self.mentions_item(key)).then_some(index)
    }

    /// The index parameter `expression` is built from, when it is one.
    ///
    /// The shapes a key is spelled in: the index itself, a template or a `+`
    /// that includes it, `index.toString()` and `String(index)`.
    fn index_in(&self, expression: &'ast Expression) -> Option<&'ast str> {
        match &**expression {
            ExpressionInner::Identifier { inner, .. } => {
                let name: &'ast str = &inner.name;
                (self.role(name) == Some(Param::Index)).then_some(name)
            }
            ExpressionInner::TemplateLiteral { inner, .. } => inner
                .expressions
                .iter()
                .find_map(|part| self.index_in(part)),
            ExpressionInner::Binary { inner, .. }
                if matches!(inner.operator, ast::expression::BinaryOperator::Plus) =>
            {
                self.index_in(&inner.left)
                    .or_else(|| self.index_in(&inner.right))
            }
            ExpressionInner::Call { inner, .. } => self.index_in(converted(inner)?),
            _ => None,
        }
    }

    /// Whether `expression` reads the item an iteration handed its callback,
    /// as a whole, in one of the shapes [`Self::index_in`] reads.
    fn mentions_item(&self, expression: &'ast Expression) -> bool {
        match &**expression {
            ExpressionInner::Identifier { inner, .. } => {
                self.role(&inner.name) == Some(Param::Item)
            }
            ExpressionInner::TemplateLiteral { inner, .. } => inner
                .expressions
                .iter()
                .any(|part| self.mentions_item(part)),
            ExpressionInner::Binary { inner, .. }
                if matches!(inner.operator, ast::expression::BinaryOperator::Plus) =>
            {
                self.mentions_item(&inner.left) || self.mentions_item(&inner.right)
            }
            ExpressionInner::Call { inner, .. } => {
                converted(inner).is_some_and(|value| self.mentions_item(value))
            }
            _ => false,
        }
    }

    /// What the innermost parameter called `name` is to the iteration that
    /// handed it over, when `name` is a parameter at all.
    fn role(&self, name: &str) -> Option<Param> {
        self.params
            .iter()
            .rev()
            .find(|(param, _)| *param == name)
            .map(|(_, role)| *role)
    }

    // --- react/jsx-no-duplicate-props ---------------------------------------

    /// A prop written twice on one element.
    ///
    /// A spread between the two does not excuse them: whatever the spread
    /// holds, the second written prop replaces the first.
    fn check_duplicate_props(&mut self, opening: &'ast jsx::Opening<Loc, Loc>) {
        let mut seen: Vec<Cow<'ast, str>> = Vec::new();
        for attribute in opening.attributes.iter() {
            let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
                continue;
            };
            let name = attribute_name(&attribute.name);
            if seen.contains(&name) {
                let element = element_name(&opening.name);
                self.report(
                    &attribute.loc,
                    DUPLICATE_PROPS,
                    format!(
                        "`{name}` is given twice on `<{element}>`, and only the last one reaches it; \
                         remove the one that was not meant"
                    ),
                );
            } else {
                seen.push(name);
            }
        }
    }

    // --- react/no-children-prop ---------------------------------------------

    /// `children` written as a prop.
    fn check_children_prop(&mut self, element: &'ast jsx::Element<Loc, Loc>) {
        let opening = &element.opening_element;
        let Some(prop) = attribute(opening, "children") else {
            return;
        };
        let name = element_name(&opening.name);
        let message = if element.children.1.iter().any(renders_something) {
            format!(
                "`children` is passed to `<{name}>` as a prop and also written between its tags, \
                 and the ones between the tags replace the prop, so it never renders; remove the \
                 prop"
            )
        } else {
            format!(
                "pass `children` to `<{name}>` between its tags rather than as a prop, which is \
                 where a reader of JSX looks for them"
            )
        };
        self.report(&prop.loc, CHILDREN_PROP, message);
    }

    // --- react/void-dom-elements-no-children --------------------------------

    /// A void element given something to hold.
    fn check_void_children(&mut self, name: &str, element: &'ast jsx::Element<Loc, Loc>) {
        if !VOID_ELEMENTS.contains(name) {
            return;
        }
        let opening = &element.opening_element;
        let held = if element.children.1.iter().any(renders_something) {
            "children"
        } else if attribute(opening, "children").is_some() {
            "a `children` prop"
        } else if attribute(opening, "dangerouslySetInnerHTML").is_some() {
            "`dangerouslySetInnerHTML`"
        } else {
            return;
        };
        self.report(&opening.loc, VOID_CHILDREN, void_message(name, held));
    }

    // --- createElement and cloneElement -------------------------------------

    /// The three rules that read `React.createElement` and
    /// `React.cloneElement` as well as JSX.
    fn check_element_call(&mut self, loc: &'ast Loc, call: &'ast ast::expression::Call<Loc, Loc>) {
        let Some(api) = element_api(&call.callee) else {
            return;
        };
        let arguments = &call.arguments.arguments;
        let props = match arguments.get(1) {
            Some(ExpressionOrSpread::Expression(props)) => match &**props {
                ExpressionInner::Object { inner, .. } => Some(&**inner),
                _ => None,
            },
            _ => None,
        };

        if self.index_key
            && let Some(props) = props
            && let Some((at, value)) = object_property(props, "key")
            && let Some(index) = self.position_key(value)
        {
            self.report(at, ARRAY_INDEX_KEY, index_message(index));
        }
        if api != ElementApi::Create {
            return;
        }
        if self.children_prop
            && let Some(props) = props
            && let Some((at, _)) = object_property(props, "children")
        {
            self.report(
                at,
                CHILDREN_PROP,
                String::from(
                    "pass `children` to `createElement` as the arguments after the props rather \
                     than as a prop, which is where a reader looks for them",
                ),
            );
        }
        if self.void_children
            && let Some(ExpressionOrSpread::Expression(kind)) = arguments.first()
            && let ExpressionInner::StringLiteral { inner: tag, .. } = &**kind
            && VOID_ELEMENTS.contains(&*tag.value)
        {
            let holds =
                |prop: &str| props.is_some_and(|props| object_property(props, prop).is_some());
            let held = if arguments.len() > 2 {
                "children"
            } else if holds("children") {
                "a `children` prop"
            } else if holds("dangerouslySetInnerHTML") {
                "`dangerouslySetInnerHTML`"
            } else {
                return;
            };
            self.report(loc, VOID_CHILDREN, void_message(&tag.value, held));
        }
    }

    // --- react/jsx-no-comment-textnodes -------------------------------------

    /// Text children whose line starts the way a comment does.
    ///
    /// The plugin's own test — a line of the text that begins, after
    /// whitespace, with `//` or `/*` — so `See https://example.com` and
    /// `a // b` are left alone.
    fn check_comment_text(&mut self, children: &'ast [jsx::Child<Loc, Loc>]) {
        for child in children {
            let jsx::Child::Text { loc, inner } = child else {
                continue;
            };
            let Some(at) = comment_start(&inner.raw) else {
                continue;
            };
            let (line, column) = offset_position(loc, &inner.raw, at);
            let opener = &inner.raw[at..at + 2];
            self.found.push(Finding {
                rule: COMMENT_TEXT,
                line,
                column,
                message: format!(
                    "`{opener}` between JSX tags is text, not a comment, so it shows on the page; \
                     write `{{/* … */}}` to comment it out, or `{{\"{opener} …\"}}` if the slashes \
                     are meant to be seen"
                ),
            });
        }
    }
}

/// The elements `expression` evaluates to: itself, either branch of a
/// conditional, or the right side of `&&`, `||` and `??`.
fn rendered_elements<'ast>(expression: &'ast Expression, out: &mut Vec<&'ast Expression>) {
    match &**expression {
        ExpressionInner::JSXElement { .. } | ExpressionInner::JSXFragment { .. } => {
            out.push(expression);
        }
        ExpressionInner::Conditional { inner, .. } => {
            rendered_elements(&inner.consequent, out);
            rendered_elements(&inner.alternate, out);
        }
        ExpressionInner::Logical { inner, .. } => rendered_elements(&inner.right, out),
        _ => {}
    }
}

/// Every element a function returns: its concise body, or the argument of each
/// `return` in its block, outside any function nested in it.
fn returned_elements<'ast>(function: &'ast Function, out: &mut Vec<&'ast Expression>) {
    match &function.body {
        ast::function::Body::BodyExpression(expression) => rendered_elements(expression, out),
        ast::function::Body::BodyBlock((_, block)) => returns_in(&block.body, out),
    }
}

fn returns_in<'ast>(statements: &'ast [Statement], out: &mut Vec<&'ast Expression>) {
    for statement in statements {
        return_in(statement, out);
    }
}

/// The `return`s inside one statement, through every statement that holds
/// others but not through a function, whose returns are its own.
fn return_in<'ast>(statement: &'ast Statement, out: &mut Vec<&'ast Expression>) {
    use ast::statement::StatementInner;

    match &**statement {
        StatementInner::Return { inner, .. } => {
            if let Some(argument) = &inner.argument {
                rendered_elements(argument, out);
            }
        }
        StatementInner::Block { inner, .. } => returns_in(&inner.body, out),
        StatementInner::If { inner, .. } => {
            return_in(&inner.consequent, out);
            if let Some(alternate) = &inner.alternate {
                return_in(&alternate.body, out);
            }
        }
        StatementInner::Switch { inner, .. } => {
            for case in inner.cases.iter() {
                returns_in(&case.consequent, out);
            }
        }
        StatementInner::Try { inner, .. } => {
            returns_in(&inner.block.1.body, out);
            if let Some(handler) = &inner.handler {
                returns_in(&handler.body.1.body, out);
            }
            if let Some((_, finalizer)) = &inner.finalizer {
                returns_in(&finalizer.body, out);
            }
        }
        StatementInner::Labeled { inner, .. } => return_in(&inner.body, out),
        StatementInner::For { inner, .. } => return_in(&inner.body, out),
        StatementInner::ForIn { inner, .. } => return_in(&inner.body, out),
        StatementInner::ForOf { inner, .. } => return_in(&inner.body, out),
        StatementInner::While { inner, .. } => return_in(&inner.body, out),
        StatementInner::DoWhile { inner, .. } => return_in(&inner.body, out),
        _ => {}
    }
}

/// The callback of a call that builds a list of elements, with the name to
/// report it under: `map` and `flatMap` on anything but `React.Children`, and
/// `Array.from`'s second argument.
fn list_callback(call: &ast::expression::Call<Loc, Loc>) -> Option<(&'static str, &Expression)> {
    let member = member_of(&call.callee)?;
    let (method, at) = match property_name(member)? {
        "map" if !is_children_api(&member.object) => ("map", 0),
        "flatMap" => ("flatMap", 0),
        "from" if is_identifier(&member.object, "Array") => ("Array.from", 1),
        _ => return None,
    };
    match call.arguments.arguments.get(at)? {
        ExpressionOrSpread::Expression(callback) => Some((method, callback)),
        ExpressionOrSpread::Spread(_) => None,
    }
}

/// What a callback parameter is to the iteration that calls the callback.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Param {
    /// The item the iteration is at.
    Item,
    /// That item's position.
    Index,
    /// Anything else, including every parameter of a function that is not an
    /// iteration's callback.
    Other,
}

/// Where an iteration method's callback is, and where its parameters are.
#[derive(Clone, Copy)]
struct Iteration {
    /// Which argument of the call is the callback.
    callback: usize,
    /// Which parameter of the callback is the item.
    item: usize,
    /// Which parameter of the callback is the index.
    index: usize,
}

/// The iteration a call makes, when its callee is an iteration method.
fn iteration_of(callee: &Expression) -> Option<Iteration> {
    let member = member_of(callee)?;
    let method = property_name(member)?;
    let at = |callback, item, index| Iteration {
        callback,
        item,
        index,
    };
    if is_children_api(&member.object) {
        return matches!(method, "map" | "forEach").then(|| at(1, 0, 1));
    }
    match method {
        "every" | "filter" | "find" | "findIndex" | "findLast" | "findLastIndex" | "flatMap"
        | "forEach" | "map" | "some" => Some(at(0, 0, 1)),
        "reduce" | "reduceRight" => Some(at(0, 1, 2)),
        _ => None,
    }
}

/// The value `x.toString()` or `String(x)` converts, when `call` is one.
fn converted(call: &ast::expression::Call<Loc, Loc>) -> Option<&Expression> {
    if let Some(member) = member_of(&call.callee)
        && property_name(member) == Some("toString")
        && call.arguments.arguments.is_empty()
    {
        return Some(&member.object);
    }
    match &*call.arguments.arguments {
        [ExpressionOrSpread::Expression(argument)] if is_identifier(&call.callee, "String") => {
            Some(argument)
        }
        _ => None,
    }
}

/// Whether a host element keeps nothing across renders that a key could carry
/// to the wrong item: anything but a form control, a media or embedded
/// element, a disclosure, a dialog or a canvas.
fn holds_no_state(tag: &str) -> bool {
    !STATEFUL_ELEMENTS.contains(tag)
}

/// Host elements that keep state of their own between renders.
///
/// `button` is one of them for the focus it holds: it is the empty host
/// element a list is most likely to be built out of — a row of dots, a grid of
/// swatches — and the browser keeps focus on the node rather than on whatever
/// the node stood for, so a keyboard user who reorders the list is left
/// focused on another item's button.
static STATEFUL_ELEMENTS: phf::Set<&'static str> = phf::phf_set! {
    "audio", "button", "canvas", "details", "dialog", "embed", "iframe", "input", "object",
    "select", "textarea", "video",
};

/// `React.createElement`, `React.cloneElement`, or either imported bare.
fn element_api(callee: &Expression) -> Option<ElementApi> {
    let name = match &**callee {
        ExpressionInner::Identifier { inner, .. } => &*inner.name,
        _ => {
            let member = member_of(callee)?;
            if !is_identifier(&member.object, "React") {
                return None;
            }
            property_name(member)?
        }
    };
    match name {
        "createElement" => Some(ElementApi::Create),
        "cloneElement" => Some(ElementApi::Clone),
        _ => None,
    }
}

/// `React.Children`, or `Children` imported from React.
fn is_children_api(expression: &Expression) -> bool {
    if is_identifier(expression, "Children") {
        return true;
    }
    member_of(expression).is_some_and(|member| {
        is_identifier(&member.object, "React") && property_name(member) == Some("Children")
    })
}

fn member_of(expression: &Expression) -> Option<&ast::expression::Member<Loc, Loc>> {
    match &**expression {
        ExpressionInner::Member { inner, .. } => Some(inner),
        ExpressionInner::OptionalMember { inner, .. } => Some(&inner.member),
        _ => None,
    }
}

fn property_name(member: &ast::expression::Member<Loc, Loc>) -> Option<&str> {
    match &member.property {
        ast::expression::member::Property::PropertyIdentifier(identifier) => Some(&identifier.name),
        _ => None,
    }
}

fn is_identifier(expression: &Expression, name: &str) -> bool {
    matches!(&**expression, ExpressionInner::Identifier { inner, .. } if &*inner.name == name)
}

fn as_function(expression: &Expression) -> Option<&Function> {
    match &**expression {
        ExpressionInner::ArrowFunction { inner, .. } | ExpressionInner::Function { inner, .. } => {
            Some(inner)
        }
        _ => None,
    }
}

/// A `key` written as a string, with where the attribute is.
fn literal_key(node: &Expression) -> Option<(&Loc, &str)> {
    let ExpressionInner::JSXElement { inner, .. } = &**node else {
        return None;
    };
    let key = attribute(&inner.opening_element, "key")?;
    match &key.value {
        Some(jsx::attribute::Value::StringLiteral((_, literal))) => {
            Some((&key.loc, &literal.value))
        }
        Some(jsx::attribute::Value::ExpressionContainer((_, container))) => {
            match &container.expression {
                jsx::expression_container::Expression::Expression(value) => match &**value {
                    ExpressionInner::StringLiteral { inner, .. } => Some((&key.loc, &inner.value)),
                    _ => None,
                },
                jsx::expression_container::Expression::EmptyExpression => None,
            }
        }
        None => None,
    }
}

/// Where a `key` is written inside a `{...{ key: … }}` object literal.
fn key_in_spread_object(opening: &jsx::Opening<Loc, Loc>) -> Option<&Loc> {
    opening.attributes.iter().find_map(|attribute| {
        let jsx::OpeningAttribute::SpreadAttribute(spread) = attribute else {
            return None;
        };
        let ExpressionInner::Object { inner, .. } = &*spread.argument else {
            return None;
        };
        object_property(inner, "key").map(|(loc, _)| loc)
    })
}

/// The property called `name` in an object literal, with where it is written.
fn object_property<'a>(
    object: &'a ast::expression::Object<Loc, Loc>,
    name: &str,
) -> Option<(&'a Loc, &'a Expression)> {
    object.properties.iter().find_map(|property| {
        let ast::expression::object::Property::NormalProperty(
            ast::expression::object::NormalProperty::Init {
                loc, key, value, ..
            },
        ) = property
        else {
            return None;
        };
        let written = match key {
            ast::expression::object::Key::Identifier(identifier) => &*identifier.name,
            ast::expression::object::Key::StringLiteral((_, literal)) => &*literal.value,
            _ => return None,
        };
        (written == name).then_some((loc, value))
    })
}

/// Whether a child puts anything on the page.
///
/// Whitespace that runs across a line break is dropped by JSX, and an empty
/// `{}` or a `{/* comment */}` holds nothing; everything else is a child.
fn renders_something(child: &jsx::Child<Loc, Loc>) -> bool {
    match child {
        jsx::Child::Text { inner, .. } => {
            !(inner.value.contains('\n') && inner.value.trim().is_empty())
        }
        jsx::Child::ExpressionContainer { inner, .. } => !matches!(
            inner.expression,
            jsx::expression_container::Expression::EmptyExpression
        ),
        _ => true,
    }
}

/// A prop's name as it is written, `xlink:href` included.
fn attribute_name(name: &jsx::attribute::Name<Loc, Loc>) -> Cow<'_, str> {
    match name {
        jsx::attribute::Name::Identifier(identifier) => Cow::Borrowed(&identifier.name),
        jsx::attribute::Name::NamespacedName(namespaced) => Cow::Owned(format!(
            "{}:{}",
            &*namespaced.namespace.name, &*namespaced.name.name
        )),
    }
}

/// An element's name as it is written: `li`, `Icons.Chevron`, `svg:rect`.
fn element_name(name: &jsx::Name<Loc, Loc>) -> String {
    match name {
        jsx::Name::Identifier(identifier) => String::from(&*identifier.name),
        jsx::Name::NamespacedName(namespaced) => {
            format!("{}:{}", &*namespaced.namespace.name, &*namespaced.name.name)
        }
        jsx::Name::MemberExpression(member) => member_name(member),
    }
}

fn member_name(member: &jsx::MemberExpression<Loc, Loc>) -> String {
    let object = match &member.object {
        jsx::member_expression::Object::Identifier(identifier) => String::from(&*identifier.name),
        jsx::member_expression::Object::MemberExpression(inner) => member_name(inner),
    };
    format!("{object}.{}", &*member.property.name)
}

fn index_message(index: &str) -> String {
    format!(
        "`key` is built from the list index `{index}`, so React ties each item's state to its \
         position and hands it to a different item when the list is reordered, filtered or added \
         to at the front; key it by something the item carries, such as its id"
    )
}

fn void_message(name: &str, held: &str) -> String {
    format!(
        "`<{name}>` is a void element and cannot hold {held}: React throws when it renders one that \
         does; put the content next to it instead"
    )
}

/// Byte offset of the first line of `text` that starts, after whitespace, with
/// `//` or `/*`.
fn comment_start(text: &str) -> Option<usize> {
    let mut line_start = 0;
    for line in text.split('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            return Some(line_start + (line.len() - trimmed.len()));
        }
        line_start += line.len() + 1;
    }
    None
}

/// The line and byte column of the byte `at` of a text node that starts at
/// `loc`.
fn offset_position(loc: &Loc, text: &str, at: usize) -> (i32, i32) {
    let clamp = |value: usize| i32::try_from(value).unwrap_or(i32::MAX);
    let before = &text[..at];
    match before.rfind('\n') {
        None => (loc.start.line, loc.start.column.saturating_add(clamp(at))),
        Some(newline) => {
            let lines = before.bytes().filter(|byte| *byte == b'\n').count();
            (
                loc.start.line.saturating_add(clamp(lines)),
                clamp(at - newline - 1),
            )
        }
    }
}
