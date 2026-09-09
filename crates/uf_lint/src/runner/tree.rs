//! The rules that read the module's syntax tree: the accessibility set, the
//! markup-nesting rule, and the one Vite rule.
//!
//! Every other rule in this crate answers its question from [`FileScan`] — a
//! line, the part of it that is code, the brace depth it opens at. These
//! cannot, and the reason is the same one each time: the question is about a
//! *node's relationship to another node*, and a line has no relationships.
//!
//! * `a11y/alt-text` has to know that an `<img>` carries no `alt` **and** no
//!   `{...spread}` that might be carrying one.
//! * `a11y/label-has-associated-control` has to know what a `<label>`'s
//!   children are.
//! * `markup/no-invalid-nesting` has to know which elements a `<div>` is
//!   inside, through however many wrappers, and — the part a text search
//!   cannot do at all — which of those wrappers is a component whose rendered
//!   shape this module does not show.
//! * `vite/hot-needs-optional-chaining` has to tell `import.meta.hot.accept`
//!   from `import.meta.hot?.accept` and from a local binding, which is a
//!   question about one `?` in a chain of member accesses.
//!
//! So this runner parses. It uses the official Flow parser's own tree and the
//! port's own [`ast_visitor`](uf_flow::ast_visitor) to walk it, rather than a
//! hand-written descent: a descent written here would be a second definition
//! of Flow syntax, and it would go quietly out of date on the next
//! `tools/upstream/sync.sh` with a rule that stopped firing as the symptom.
//!
//! # The JSX walk is this module's own, and that is deliberate
//!
//! [`Tree::jsx_element`] does not call the visitor's default descent. It walks
//! a JSX element itself, because the default cannot distinguish the two places
//! a child expression can sit:
//!
//! ```text
//! <p>{list.map((row) => <div />)}</p>   // the <div> really is inside the <p>
//! <p footer={<div />}>text</p>          // the <div> is not inside anything
//! ```
//!
//! An attribute's value is an expression the element is *given*, not a node
//! inside it, so the ancestor stack is emptied for the length of one and put
//! back afterwards. Children keep it, through fragments and through
//! `{ … }` containers, because whatever comes out of one is rendered where it
//! was written. A **component** child pushes [`Ancestor::Opaque`] instead of a
//! name: `<p><Card><div /></Card></p>` may or may not put the `<div>` inside
//! the `<p>`, and a rule that guessed would be reporting a file it cannot see.
//!
//! # What this costs, and when it is paid
//!
//! Parsing a module again is far more work than a line scan, so four gates
//! stand in front of it, cheapest first:
//!
//! 1. None of the rules is enabled — nothing runs.
//! 2. The path is not Flow source — nothing runs.
//! 3. The text holds neither a JSX tag closer (`</`, `/>`) nor `import.meta` —
//!    nothing runs, because a module without one of those cannot violate any
//!    of these rules.
//! 4. The module is nested or chained past [`uf_flow`]'s parser ceilings —
//!    nothing runs, because `flow/syntax` has already refused it and a linter
//!    must not be the thing that overflows a stack on a minified bundle.
//!
//! Past those, the parse and the walk run on a thread of
//! [`PARSE_STACK_BYTES`](uf_flow::PARSE_STACK_BYTES), for the reason
//! `uf_flow::parse` spells out: the port's frames are large and a lint
//! worker's own stack is not enough. A module whose parse produced diagnostics
//! is dropped rather than walked — `flow/syntax` has already reported it, and
//! a recovered tree is the parser's best guess at a file nobody has fixed yet.
//!
//! # Why this is not [`super::react_tree`]'s parse
//!
//! Both runners read a tree and neither shares one, which looks like waste and
//! is not. `react_tree` needs the **Babel-shaped** tree, because its answer
//! comes from handing the module to the official React Compiler and comparing
//! what came back; these rules need the **port's own typed tree**, because
//! `jsx::Element` says which of its parts is an attribute and which is a child
//! and a JSON object says neither without a schema written here. Converting
//! one into the other is more work than parsing again, and a shared tree that
//! served both would be a third shape neither of them wants.
//!
//! So a component module that both renders JSX and calls `useEffect` is parsed
//! twice, and that is the honest cost. It is bounded by the gates above — a
//! module with no JSX tag closer and no `import.meta` never reaches this
//! runner at all — and `uf lint` over this repository, 422 files, still
//! finishes in under a second.

mod aria;

use uf_config::UniflowedConfig;
use uf_flow::ast::jsx;
use uf_flow::ast_visitor::{self, AstVisitor};
use uf_flow::{Loc, ast};
use uf_profiler::profile_span;

use crate::scan::FileScan;
use crate::{Diagnostic, Severity, push_at, severity};

/// `a11y/alt-text`.
const ALT_TEXT: &str = "a11y/alt-text";

/// `a11y/aria-props`.
const ARIA_PROPS: &str = "a11y/aria-props";

/// `a11y/heading-order`.
const HEADING_ORDER: &str = "a11y/heading-order";

/// `a11y/label-has-associated-control`.
const LABEL_CONTROL: &str = "a11y/label-has-associated-control";

/// `a11y/no-static-element-interactions`.
const STATIC_INTERACTIONS: &str = "a11y/no-static-element-interactions";

/// `markup/no-invalid-nesting`.
const INVALID_NESTING: &str = "markup/no-invalid-nesting";

/// `vite/hot-needs-optional-chaining`.
const HOT_OPTIONAL_CHAINING: &str = "vite/hot-needs-optional-chaining";

/// Report the rules that need the module's tree.
pub(crate) fn run_tree_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    profile_span!("run_tree_rules");
    let levels = Levels::for_config(config);
    if levels.all_off() {
        return;
    }
    if !super::flow_syntax::is_flow_syntax_target(&scan.file.path) {
        return;
    }

    let source = &scan.file.source;
    // A JSX element either closes (`</p>`) or closes itself (`/>`); a module
    // with neither has no JSX in it. `import.meta` is the other rule's whole
    // subject. Both are substring tests over the raw source, so a mention
    // inside a comment or a string costs one parse and no diagnostic.
    let wants_jsx = levels.any_jsx() && (source.contains("</") || source.contains("/>"));
    let wants_hot = levels.hot_optional_chaining.is_some() && source.contains("import.meta");
    if !wants_jsx && !wants_hot {
        return;
    }

    // The same ceilings `uf_flow::parse` applies before it parses, asked here
    // so that a module over one of them costs a scan rather than a parse. It
    // has already been reported by `flow/syntax`.
    let depths = uf_flow::depths(source);
    if source.len() > uf_flow::MAX_PARSE_BYTES
        || depths.brackets > uf_flow::MAX_NESTING_DEPTH
        || depths.chain > uf_flow::MAX_CHAIN_DEPTH
    {
        return;
    }

    let Some(found) = analyse(source, &levels, wants_hot) else {
        return;
    };

    for finding in found {
        let Some(severity) = levels.of(finding.rule) else {
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
        // The port counts columns in bytes from the start of the line, which
        // is what `push_at` wants — but a position past the end of the line
        // would resolve into the *next* line, so it is clamped rather than
        // trusted.
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
    alt_text: Option<Severity>,
    aria_props: Option<Severity>,
    heading_order: Option<Severity>,
    label_control: Option<Severity>,
    static_interactions: Option<Severity>,
    invalid_nesting: Option<Severity>,
    hot_optional_chaining: Option<Severity>,
}

impl Levels {
    fn for_config(config: &UniflowedConfig) -> Self {
        Self {
            alt_text: severity(config, ALT_TEXT),
            aria_props: severity(config, ARIA_PROPS),
            heading_order: severity(config, HEADING_ORDER),
            label_control: severity(config, LABEL_CONTROL),
            static_interactions: severity(config, STATIC_INTERACTIONS),
            invalid_nesting: severity(config, INVALID_NESTING),
            hot_optional_chaining: severity(config, HOT_OPTIONAL_CHAINING),
        }
    }

    fn all_off(&self) -> bool {
        !self.any_jsx() && self.hot_optional_chaining.is_none()
    }

    /// Whether any rule that reads JSX is on.
    fn any_jsx(&self) -> bool {
        self.alt_text.is_some()
            || self.aria_props.is_some()
            || self.heading_order.is_some()
            || self.label_control.is_some()
            || self.static_interactions.is_some()
            || self.invalid_nesting.is_some()
    }

    fn of(&self, rule: &str) -> Option<Severity> {
        match rule {
            ALT_TEXT => self.alt_text,
            ARIA_PROPS => self.aria_props,
            HEADING_ORDER => self.heading_order,
            LABEL_CONTROL => self.label_control,
            STATIC_INTERACTIONS => self.static_interactions,
            INVALID_NESTING => self.invalid_nesting,
            HOT_OPTIONAL_CHAINING => self.hot_optional_chaining,
            _ => None,
        }
    }
}

/// One finding, positioned the way the port positions nodes.
struct Finding {
    rule: &'static str,
    /// 1-based line.
    line: i32,
    /// 0-based byte column within that line.
    column: i32,
    message: String,
}

/// Parse the module and walk it, or [`None`] when there is no tree to walk.
///
/// [`None`] covers a module over a parser ceiling and a module with syntax
/// errors alike. `flow/syntax` reports the second, and a recovered tree is the
/// parser's guess at what the author meant — reporting an `<img>` the parser
/// invented while recovering would be reporting a file that does not exist.
fn analyse(source: &str, levels: &Levels, wants_hot: bool) -> Option<Vec<Finding>> {
    let work = || {
        let parsed = uf_flow::parse(source).ok()?;
        if !parsed.is_ok() {
            return None;
        }
        let mut tree = Tree {
            hot: wants_hot,
            alt_text: levels.alt_text.is_some(),
            aria_props: levels.aria_props.is_some(),
            heading_order: levels.heading_order.is_some(),
            label_control: levels.label_control.is_some(),
            static_interactions: levels.static_interactions.is_some(),
            invalid_nesting: levels.invalid_nesting.is_some(),
            ancestors: Vec::new(),
            heading: None,
            found: Vec::new(),
        };
        // Nothing in the walk fails, so the `Result` the visitor's signature
        // carries has no error to report; it exists for visitors that stop
        // early, and this one never does.
        let _ = tree.program(&parsed.program);
        let mut found = tree.found;
        found.sort_by_key(|finding| (finding.line, finding.column, finding.rule));
        Some(found)
    };

    // The port's frames are large and the walk below recurses once per level
    // of the tree, so both go on a thread with the stack `uf_flow` documents
    // for its own ceilings. `Parsed` takes a deep tree to its own thread to
    // free it, so nothing is carried back onto a small stack.
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("uf-lint-tree".into())
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, work)
            .ok()?
            .join()
            .ok()?
    })
}

/// What stands between a JSX element and the elements above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ancestor<'a> {
    /// A host element — `<p>`, `<div>` — which is a DOM node the HTML parser
    /// will see, spelled exactly as it will see it.
    Host(&'a str),
    /// A component, or anything else whose rendered shape this module does not
    /// show. Nothing above it is an ancestor of anything below it.
    Opaque,
}

/// The walk: one pass over the tree, carrying what the rules need to know.
struct Tree<'a> {
    hot: bool,
    alt_text: bool,
    aria_props: bool,
    heading_order: bool,
    label_control: bool,
    static_interactions: bool,
    invalid_nesting: bool,
    /// The JSX elements enclosing the node being visited, outermost first.
    ancestors: Vec<Ancestor<'a>>,
    /// Level of the last heading seen in the function being visited.
    heading: Option<u8>,
    found: Vec<Finding>,
}

impl<'ast> AstVisitor<'ast, Loc, Loc, &'ast Loc, ()> for Tree<'ast> {
    fn normalize_loc(loc: &'ast Loc) -> &'ast Loc {
        loc
    }

    fn normalize_type(type_: &'ast Loc) -> &'ast Loc {
        type_
    }

    /// `vite/hot-needs-optional-chaining`, and then the ordinary descent.
    ///
    /// A `Member` node is the non-optional `.x`; `import.meta.hot?.accept` is
    /// an `OptionalMember` and is never matched here, which is what makes the
    /// rule silent on the two forms that work.
    fn expression(&mut self, expr: &'ast ast::expression::Expression<Loc, Loc>) -> Result<(), ()> {
        if self.hot
            && let ast::expression::ExpressionInner::Member { loc, inner } = &**expr
            && is_import_meta_hot(&inner.object)
        {
            self.hot_without_optional_chaining(loc, &inner.property);
        }
        ast_visitor::expression_default(self, expr)
    }

    /// A JSX element, walked by this module rather than by the default.
    ///
    /// See the module documentation for why: an attribute's value is not a
    /// child, and a component child is a wall the ancestor rules stop at.
    fn jsx_element(
        &mut self,
        _loc: &'ast Loc,
        element: &'ast jsx::Element<Loc, Loc>,
    ) -> Result<(), ()> {
        let opening = &element.opening_element;
        if self.aria_props {
            self.check_aria_props(opening);
        }
        let tag = host_name(&opening.name);
        if let Some(name) = tag {
            if self.alt_text {
                self.check_alt_text(name, opening);
            }
            if self.static_interactions {
                self.check_static_interactions(name, opening);
            }
            if self.label_control {
                self.check_label_control(name, element);
            }
            if self.heading_order {
                self.check_heading_order(name, opening);
            }
            if self.invalid_nesting {
                self.check_nesting(name, opening);
            }
        }

        // An attribute value is an expression handed to this element, not a
        // node inside it.
        let outer = std::mem::take(&mut self.ancestors);
        for attribute in &*opening.attributes {
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
        self.ancestors = outer;

        self.ancestors
            .push(tag.map_or(Ancestor::Opaque, Ancestor::Host));
        let walked = self.children(&element.children.1);
        self.ancestors.pop();
        walked
    }

    /// A fragment renders no node, so it is transparent to the ancestor rules.
    fn jsx_fragment(
        &mut self,
        _loc: &'ast Loc,
        fragment: &'ast jsx::Fragment<Loc, Loc>,
    ) -> Result<(), ()> {
        self.children(&fragment.frag_children.1)
    }

    /// A function body is where `a11y/heading-order` starts over.
    ///
    /// Heading levels are a claim about one rendered document, and a module
    /// holds as many as it holds functions: a `<h1>` in a layout and a `<h3>`
    /// in a card below it in the same file are not a skip, because nothing
    /// says the card is rendered under the layout. Comparing only within one
    /// function body is the part that is certain.
    fn function_(
        &mut self,
        loc: &'ast Loc,
        expr: &'ast ast::function::Function<Loc, Loc>,
    ) -> Result<(), ()> {
        let outer = self.heading.take();
        let walked = ast_visitor::function_default(self, loc, expr);
        self.heading = outer;
        walked
    }

    fn component_declaration(
        &mut self,
        loc: &'ast Loc,
        component: &'ast ast::statement::ComponentDeclaration<Loc, Loc>,
    ) -> Result<(), ()> {
        let outer = self.heading.take();
        let walked = ast_visitor::component_declaration_default(self, loc, component);
        self.heading = outer;
        walked
    }
}

impl<'ast> Tree<'ast> {
    /// Walk JSX children, keeping the ancestor stack: whatever a `{ … }`
    /// produces is rendered where it was written.
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

    // --- a11y/alt-text ------------------------------------------------------

    /// An image with no text alternative.
    ///
    /// Reported for `<img>`, `<area>` and `<input type="image">`, which is the
    /// whole of the set whose answer is unambiguous: each one *is* content in
    /// the page, and a reader that reaches one with no `alt` announces the URL
    /// or nothing at all.
    ///
    /// **Deliberately silent** on an element carrying `{...props}`, because
    /// the `alt` may be in there and this rule cannot see inside a spread; and
    /// on `<input>` whose `type` is an expression, because a rule about
    /// `type="image"` must not fire on a text field. Both cost a diagnostic
    /// somebody may want and neither invents one, which is the direction a
    /// linter should be wrong in.
    fn check_alt_text(&mut self, name: &str, opening: &'ast jsx::Opening<Loc, Loc>) {
        let carries_content = match name {
            "img" | "area" => true,
            "input" => string_attribute(opening, "type") == Some("image"),
            _ => false,
        };
        if !carries_content || has_spread(opening) || attribute(opening, "alt").is_some() {
            return;
        }

        self.report(
            &opening.loc,
            ALT_TEXT,
            format!(
                "`<{name}>` has no `alt`, so a screen reader announces its URL or nothing at all; \
                 give it the words the image is carrying, or `alt=\"\"` when it carries none and \
                 the page already says them"
            ),
        );
    }

    // --- a11y/aria-props ----------------------------------------------------

    /// An `aria-*` attribute that is not one.
    ///
    /// On components as well as host elements: a component that takes an
    /// `aria-*` prop is taking it to put on a DOM node, and a misspelling is
    /// as inert one level up as it is at the bottom.
    fn check_aria_props(&mut self, opening: &'ast jsx::Opening<Loc, Loc>) {
        for attribute in &*opening.attributes {
            let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
                continue;
            };
            let jsx::attribute::Name::Identifier(name) = &attribute.name else {
                continue;
            };
            let name = &*name.name;
            if !name.starts_with("aria-") || aria::is_aria_attribute(name) {
                continue;
            }

            let suggestion = aria::nearest_aria_attribute(name)
                .map(|near| format!("; did you mean `{near}`?"))
                .unwrap_or_default();
            self.report(
                &attribute.loc,
                ARIA_PROPS,
                format!(
                    "`{name}` is not an ARIA attribute, so nothing reads it: the browser keeps it, \
                     no assistive technology looks at it, and the element stays unlabelled with no \
                     symptom to notice{suggestion}"
                ),
            );
        }
    }

    // --- a11y/no-static-element-interactions --------------------------------

    /// A click handler on something a keyboard cannot reach.
    ///
    /// The narrow half of what `eslint-plugin-jsx-a11y` splits between
    /// `no-static-element-interactions` and `click-events-have-key-events`:
    /// reported only when the element is a host element that is not
    /// interactive, has an `onClick`, and has **neither** a `role` **nor** a
    /// key handler. Either one is a statement that somebody thought about the
    /// keyboard, and a rule at `error` should not be the one to argue about
    /// whether they thought hard enough.
    ///
    /// **Deliberately silent** on an element with `{...props}`, which may be
    /// spreading either of them in.
    fn check_static_interactions(&mut self, name: &str, opening: &'ast jsx::Opening<Loc, Loc>) {
        if INTERACTIVE_ELEMENTS.contains(name)
            || has_spread(opening)
            || attribute(opening, "onClick").is_none()
            || attribute(opening, "role").is_some()
            || KEY_HANDLERS
                .iter()
                .any(|handler| attribute(opening, handler).is_some())
        {
            return;
        }

        self.report(
            &opening.loc,
            STATIC_INTERACTIONS,
            format!(
                "`<{name}>` has an `onClick` but no `role` and no key handler, so nothing but a \
                 mouse can reach it; make it a `<button>`, or give it a `role` and an `onKeyDown`"
            ),
        );
    }

    // --- a11y/label-has-associated-control ----------------------------------

    /// A `<label>` that labels nothing.
    ///
    /// Reported only for the shape that is certainly wrong: no `htmlFor`, no
    /// `{...props}`, and children that are nothing but text. A label with an
    /// element inside it may hold the control; a label with a `{ … }` child
    /// may hold anything at all. Neither is something to guess about, so
    /// neither is reported.
    fn check_label_control(&mut self, name: &str, element: &'ast jsx::Element<Loc, Loc>) {
        let opening = &element.opening_element;
        if name != "label" || has_spread(opening) || attribute(opening, "htmlFor").is_some() {
            return;
        }
        let only_text = element
            .children
            .1
            .iter()
            .all(|child| matches!(child, jsx::Child::Text { .. }));
        if !only_text {
            return;
        }

        self.report(
            &opening.loc,
            LABEL_CONTROL,
            String::from(
                "this `<label>` is attached to no control, so clicking it does nothing and the \
                 field it describes has no accessible name; point `htmlFor` at the control's `id`, \
                 or put the control inside the label",
            ),
        );
    }

    // --- a11y/heading-order -------------------------------------------------

    /// A heading level that skips one.
    ///
    /// A reader navigating by heading uses the levels as an outline, and a gap
    /// in it reads as a section that failed to load. Compared within one
    /// function body only; see [`Tree::function_`].
    fn check_heading_order(&mut self, name: &str, opening: &'ast jsx::Opening<Loc, Loc>) {
        let Some(level) = heading_level(name) else {
            return;
        };
        let previous = self.heading.replace(level);
        let Some(previous) = previous else {
            return;
        };
        if level <= previous + 1 {
            return;
        }

        let wanted = previous + 1;
        self.report(
            &opening.loc,
            HEADING_ORDER,
            format!(
                "heading level jumps from `<h{previous}>` to `<{name}>`, and a reader navigating \
                 by heading reads the gap as a section that is missing; use `<h{wanted}>` and give \
                 it the size you wanted with CSS"
            ),
        );
    }

    // --- markup/no-invalid-nesting ------------------------------------------

    /// Markup the HTML parser will not leave where it was written.
    ///
    /// This is a hydration rule rather than a style rule. React renders the
    /// tree the module describes; the browser builds the tree the HTML parser
    /// allows, and where the two differ React finds a mismatch it has to
    /// repair. `<p><div /></p>` is the canonical one: the parser closes the
    /// `<p>` at the `<div>`, so the browser has two siblings where React has a
    /// parent and a child.
    ///
    /// **Deliberately silent** once the path from the element to the ancestor
    /// passes through a component ([`Ancestor::Opaque`]) or through an
    /// attribute value, because neither says where the node is rendered; and
    /// once a `<p>`'s paragraph has already been closed by something nearer,
    /// so one broken structure is one diagnostic rather than one per level.
    fn check_nesting(&mut self, name: &str, opening: &'ast jsx::Opening<Loc, Loc>) {
        let Some((parent, rewrite)) = self.rewritten_by_the_parser(name) else {
            return;
        };

        self.report(
            &opening.loc,
            INVALID_NESTING,
            format!(
                "`<{name}>` inside `<{parent}>` is not markup a browser keeps: {rewrite}. React \
                 renders one tree and the parser builds another, which is a hydration mismatch \
                 rather than a matter of taste"
            ),
        );
    }

    /// The ancestor that makes `name` illegal, and what the parser does about it.
    fn rewritten_by_the_parser(&self, name: &str) -> Option<(&'ast str, &'static str)> {
        // The table rules are about the immediate parent: `<td>` is fine two
        // levels under a `<table>` and wrong directly under one.
        if let Some(Ancestor::Host(parent)) = self.ancestors.last() {
            if let Some(allowed) = table_children(parent)
                && !allowed.contains(&name)
            {
                return Some((parent, "the parser moves it out of the table, ahead of it"));
            }
            if *parent == "li" && name == "li" {
                return Some((parent, "the parser closes the outer `<li>` at it"));
            }
        }

        // `<a>`, `<button>` and `<form>` may not contain themselves at any
        // depth, and no intervening element makes that legal.
        if matches!(name, "a" | "button" | "form")
            && let Some(parent) = self.nearest_host(|host| host == name)
        {
            let rewrite = match name {
                "form" => "the parser drops the inner `<form>` and keeps its contents",
                _ => "the parser closes the outer one at it, so they end up as siblings",
            };
            return Some((parent, rewrite));
        }

        // A block-level start tag closes an open `<p>` — but only the nearest
        // one, and only if nothing between here and it has closed it already.
        // The search therefore stops at the first ancestor that is itself a
        // paragraph-closing tag: past that, whatever `<p>` is further out was
        // closed there and this element is not inside it. `<p><div><ul>…` is
        // one break and one diagnostic, not one per level.
        if CLOSES_A_PARAGRAPH.contains(name)
            && let Some(parent) = self.nearest_host(|host| CLOSES_A_PARAGRAPH.contains(host))
            && parent == "p"
        {
            return Some((parent, "the parser closes the `<p>` before it"));
        }

        None
    }

    /// The nearest enclosing host element `wanted` accepts, stopping at the
    /// first [`Ancestor::Opaque`].
    fn nearest_host(&self, wanted: impl Fn(&str) -> bool) -> Option<&'ast str> {
        for ancestor in self.ancestors.iter().rev() {
            match ancestor {
                Ancestor::Opaque => return None,
                Ancestor::Host(host) if wanted(host) => return Some(host),
                Ancestor::Host(_) => {}
            }
        }
        None
    }

    // --- vite/hot-needs-optional-chaining -----------------------------------

    /// A property read straight off `import.meta.hot`.
    ///
    /// The message carries the second sentence Flow cannot say. Flow's own
    /// error — `property accept is missing in undefined` — is correct about a
    /// build, in which Vite replaces `import.meta.hot` with `undefined`; what
    /// it does not say is that the guard every Vite guide writes does not help,
    /// because Flow keys a refinement by a lookup rooted at an identifier and
    /// a meta-property is not one. So `if (import.meta.hot)` refines nothing
    /// and the reader is left with a correct error about correct-looking code.
    /// See ubugeeei-prod/uf#320.
    fn hot_without_optional_chaining(
        &mut self,
        loc: &Loc,
        property: &ast::expression::member::Property<Loc, Loc>,
    ) {
        let read = match property {
            ast::expression::member::Property::PropertyIdentifier(identifier) => {
                format!("`import.meta.hot.{}`", &*identifier.name)
            }
            _ => String::from("`import.meta.hot`"),
        };
        self.report(
            loc,
            HOT_OPTIONAL_CHAINING,
            format!(
                "`import.meta.hot` is undefined in a build, and `if (import.meta.hot)` does not \
                 refine it: Flow cannot key a refinement on `import.meta`. Write {read} with `?.`, \
                 or bind `const hot = import.meta.hot;` first and test that"
            ),
        );
    }
}

/// Whether `expression` is `import.meta.hot`.
fn is_import_meta_hot(expression: &ast::expression::Expression<Loc, Loc>) -> bool {
    let ast::expression::ExpressionInner::Member { inner, .. } = &**expression else {
        return false;
    };
    let ast::expression::member::Property::PropertyIdentifier(property) = &inner.property else {
        return false;
    };
    if &*property.name != "hot" {
        return false;
    }
    matches!(
        &*inner.object,
        ast::expression::ExpressionInner::MetaProperty { inner, .. }
            if &*inner.meta.name == "import" && &*inner.property.name == "meta"
    )
}

/// The tag of a JSX element when it names a host element.
///
/// [`None`] for a component (`<Card>`, `<Icons.Chevron>`) and for a namespaced
/// name (`<svg:use>`, which uf does not serve). The test is React's own: a
/// name that starts with a lowercase letter is an HTML tag and anything else
/// is a value in scope.
fn host_name(name: &jsx::Name<Loc, Loc>) -> Option<&str> {
    let jsx::Name::Identifier(identifier) = name else {
        return None;
    };
    let name = &*identifier.name;
    name.starts_with(|first: char| first.is_ascii_lowercase())
        .then_some(name)
}

/// The attribute called `wanted`, when the element has one.
fn attribute<'a>(
    opening: &'a jsx::Opening<Loc, Loc>,
    wanted: &str,
) -> Option<&'a jsx::Attribute<Loc, Loc>> {
    opening.attributes.iter().find_map(|attribute| {
        let jsx::OpeningAttribute::Attribute(attribute) = attribute else {
            return None;
        };
        let jsx::attribute::Name::Identifier(name) = &attribute.name else {
            return None;
        };
        (&*name.name == wanted).then_some(attribute)
    })
}

/// The value of `wanted`, when it is written as a string literal.
fn string_attribute<'a>(opening: &'a jsx::Opening<Loc, Loc>, wanted: &str) -> Option<&'a str> {
    match &attribute(opening, wanted)?.value {
        Some(jsx::attribute::Value::StringLiteral((_, literal))) => Some(&literal.value),
        _ => None,
    }
}

/// Whether the element carries a `{...spread}`.
fn has_spread(opening: &jsx::Opening<Loc, Loc>) -> bool {
    opening
        .attributes
        .iter()
        .any(|attribute| matches!(attribute, jsx::OpeningAttribute::SpreadAttribute(_)))
}

/// The level of `<h1>` … `<h6>`.
fn heading_level(name: &str) -> Option<u8> {
    let level = name.strip_prefix('h')?;
    match level {
        "1" => Some(1),
        "2" => Some(2),
        "3" => Some(3),
        "4" => Some(4),
        "5" => Some(5),
        "6" => Some(6),
        _ => None,
    }
}

/// What each table element may hold, for the elements whose contents the
/// parser rearranges.
///
/// Anything else in one of these is *foster-parented*: the parser takes it out
/// of the table and puts it immediately before it. `<table><tr>` is in the
/// list too — the parser inserts the `<tbody>` React did not render, which is
/// the same mismatch by a gentler route.
fn table_children(parent: &str) -> Option<&'static [&'static str]> {
    match parent {
        "table" => Some(&[
            "caption", "colgroup", "col", "thead", "tbody", "tfoot", "script", "template", "style",
        ]),
        "thead" | "tbody" | "tfoot" => Some(&["tr", "script", "template", "style"]),
        "tr" => Some(&["td", "th", "script", "template", "style"]),
        _ => None,
    }
}

/// Start tags that close an open `<p>`.
///
/// The HTML standard's "in body" insertion mode, minus the tags no browser
/// still parses specially (`center`, `dir`, `listing`, `plaintext`, `xmp`) —
/// a rule about markup people write has nothing to say about those.
static CLOSES_A_PARAGRAPH: phf::Set<&'static str> = phf::phf_set! {
    "address", "article", "aside", "blockquote", "dd", "details", "dialog", "div", "dl", "dt",
    "fieldset", "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6",
    "header", "hgroup", "hr", "li", "main", "menu", "nav", "ol", "p", "pre", "search", "section",
    "summary", "table", "ul",
};

/// Host elements a pointer and a keyboard both already reach.
///
/// `<a>` is here without asking about `href`, and that is a deliberate
/// narrowing: `<a onClick>` with no `href` is a real defect, and it is the
/// *other* rule's — one about anchors — rather than a reason for this one to
/// start reasoning about attributes it otherwise does not read.
static INTERACTIVE_ELEMENTS: phf::Set<&'static str> = phf::phf_set! {
    "a", "area", "audio", "button", "details", "embed", "iframe", "input", "label", "menuitem",
    "object", "option", "select", "summary", "textarea", "video",
};

/// The handlers that answer a key press.
const KEY_HANDLERS: [&str; 3] = ["onKeyDown", "onKeyUp", "onKeyPress"];
