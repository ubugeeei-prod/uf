//! The rules that read the module's tree: the `a11y/*` set,
//! `markup/no-invalid-nesting` and `vite/hot-needs-optional-chaining`.
//!
//! Every rule here is tested from both sides. A linter is judged by what it
//! does *not* say as much as by what it does — a rule that fires on the
//! working form is worse than no rule at all, because the file that gets
//! changed is the one that was right — so each rule has the shape it must
//! report and the shapes it must leave alone, and the second list is the
//! longer one.

use super::*;

/// A module with `component` syntax around `body`, which is where JSX lives in
/// a uf project and is also what `a11y/heading-order` scopes to.
fn component(body: &str) -> String {
    format!("// @flow\ncomponent Page() renders React.Node {{\n  return (\n{body}\n  );\n}}\n")
}

// --- a11y/alt-text ----------------------------------------------------------

#[test]
fn alt_text_reports_an_image_with_no_alt() {
    let diagnostics = lint_js("a11y/alt-text", &component("    <img src=\"/cat.png\" />"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 4);
    assert!(
        diagnostics[0].message.contains("alt=\"\""),
        "{diagnostics:?}"
    );
}

#[test]
fn alt_text_reports_an_area_and_an_image_input() {
    for markup in [
        "    <area href=\"/a\" shape=\"rect\" />",
        "    <input type=\"image\" src=\"/go.png\" />",
    ] {
        let diagnostics = lint_js("a11y/alt-text", &component(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn alt_text_accepts_every_way_an_alt_can_arrive() {
    // An empty `alt` is the answer for a decorative image and must not be
    // reported: uf's own `docs/app/$layout.js` writes one.
    for markup in [
        "    <img src=\"/cat.png\" alt=\"a cat\" />",
        "    <img src=\"/mark.svg\" alt=\"\" />",
        "    <img src={source} alt={caption} />",
        // A spread may be carrying the `alt`, and this rule cannot see inside
        // one. `packages/web/media.js` is written this way.
        "    <img src=\"/cat.png\" {...rest} />",
        // A text input is not an image, and `type` decides which this is.
        "    <input type=\"text\" name=\"q\" />",
        // An unknown `type` is not a claim that it is an image.
        "    <input type={kind} name=\"q\" />",
        // A component named `Image` renders whatever it renders.
        "    <Image src=\"/cat.png\" />",
    ] {
        let diagnostics = lint_js("a11y/alt-text", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- a11y/aria-props --------------------------------------------------------

#[test]
fn aria_props_reports_a_misspelled_attribute_and_says_what_was_meant() {
    let diagnostics = lint_js(
        "a11y/aria-props",
        &component("    <button aria-lable=\"Close\">x</button>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0]
            .message
            .contains("did you mean `aria-label`?"),
        "{diagnostics:?}"
    );
}

#[test]
fn aria_props_reports_an_invented_attribute_without_guessing() {
    let diagnostics = lint_js(
        "a11y/aria-props",
        &component("    <div aria-nonsense-attribute=\"1\">x</div>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(!diagnostics[0].message.contains("did you mean"));
}

#[test]
fn aria_props_reports_a_component_prop_too() {
    // A component takes an `aria-*` prop in order to put it on a DOM node, so
    // the misspelling is as inert one level up as it is at the bottom.
    let diagnostics = lint_js(
        "a11y/aria-props",
        &component("    <Dialog aria-describeby=\"body\">x</Dialog>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn aria_props_accepts_the_attributes_that_exist() {
    for markup in [
        "    <div aria-label=\"Menu\" aria-hidden=\"true\">x</div>",
        "    <div aria-labelledby=\"title\" aria-describedby=\"body\">x</div>",
        "    <div aria-braillelabel=\"x\" aria-rowindextext=\"first\">x</div>",
        // Deprecated, and still an ARIA attribute.
        "    <div aria-dropeffect=\"none\">x</div>",
        // React lowercases it on the way to the DOM, so this works.
        "    <div aria-Label=\"Menu\">x</div>",
        // Not an `aria-` name at all.
        "    <div data-aria-label=\"Menu\" role=\"note\">x</div>",
    ] {
        let diagnostics = lint_js("a11y/aria-props", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- a11y/no-static-element-interactions ------------------------------------

#[test]
fn static_interactions_reports_a_div_that_only_a_mouse_can_reach() {
    let diagnostics = lint_js(
        "a11y/no-static-element-interactions",
        &component("    <div onClick={open}>Open</div>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("<button>"),
        "{diagnostics:?}"
    );
}

#[test]
fn static_interactions_leaves_alone_anything_that_considered_the_keyboard() {
    for markup in [
        // The element a keyboard already reaches.
        "    <button onClick={open}>Open</button>",
        "    <a href=\"/x\" onClick={open}>Open</a>",
        "    <summary onClick={open}>Open</summary>",
        "    <input onClick={open} />",
        // A role, or a key handler: either says somebody thought about it.
        "    <div role=\"button\" tabIndex={0} onClick={open}>Open</div>",
        "    <div onClick={open} onKeyDown={open}>Open</div>",
        "    <div onClick={open} onKeyUp={open}>Open</div>",
        // A spread may be carrying either one in.
        "    <div onClick={open} {...rest}>Open</div>",
        // No handler at all.
        "    <div className=\"card\">Open</div>",
        // A component decides its own markup.
        "    <Card onClick={open}>Open</Card>",
    ] {
        let diagnostics = lint_js("a11y/no-static-element-interactions", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- a11y/label-has-associated-control --------------------------------------

#[test]
fn label_control_reports_a_label_attached_to_nothing() {
    let diagnostics = lint_js(
        "a11y/label-has-associated-control",
        &component("    <label>Email</label>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("htmlFor"),
        "{diagnostics:?}"
    );
}

#[test]
fn label_control_accepts_a_label_that_could_be_naming_something() {
    for markup in [
        // `packages/ui/field.js` and the tutorial both write this one.
        "    <label htmlFor=\"email\">Email</label>",
        "    <label htmlFor={field.controlId}>Email</label>",
        // The control is inside it.
        "    <label>Email <input name=\"email\" /></label>",
        // A component child may be the control; nothing here says it is not.
        "    <label><Field name=\"email\" /></label>",
        // An expression child may be anything at all.
        "    <label>{children}</label>",
        // A spread may be carrying `htmlFor`. `packages/ui/combobox.js` does.
        "    <label {...rest}>Email</label>",
    ] {
        let diagnostics = lint_js("a11y/label-has-associated-control", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- a11y/heading-order -----------------------------------------------------

#[test]
fn heading_order_reports_a_level_that_skips() {
    let diagnostics = lint_js(
        "a11y/heading-order",
        &component("    <section>\n      <h1>Title</h1>\n      <h3>Detail</h3>\n    </section>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 6);
    assert!(diagnostics[0].message.contains("<h2>"), "{diagnostics:?}");
}

#[test]
fn heading_order_accepts_an_outline_with_no_gaps() {
    for markup in [
        "    <section>\n      <h1>Title</h1>\n      <h2>Part</h2>\n    </section>",
        // Going back up any distance is not a skip: the outline closes
        // sections rather than opening one that is not there.
        "    <section>\n      <h1>Title</h1>\n      <h2>Part</h2>\n      <h3>Bit</h3>\n      <h1>Next</h1>\n    </section>",
        // A single heading has nothing to skip from, whatever its level.
        "    <section>\n      <h4>Aside</h4>\n    </section>",
    ] {
        let diagnostics = lint_js("a11y/heading-order", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

#[test]
fn heading_order_does_not_compare_across_two_components() {
    // A `<h1>` in a layout and a `<h3>` in a card are a skip only if the one
    // renders inside the other, and a module does not say that. Comparing
    // within one function body is the part that is certain.
    let source = "// @flow\n\
                  component Layout() renders React.Node {\n\
                  \x20 return <h1>Site</h1>;\n\
                  }\n\
                  component Card() renders React.Node {\n\
                  \x20 return <h3>Card</h3>;\n\
                  }\n";

    let diagnostics = lint_js("a11y/heading-order", source);

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

// --- markup/no-invalid-nesting ----------------------------------------------

#[test]
fn invalid_nesting_reports_a_block_inside_a_paragraph() {
    let diagnostics = lint_js(
        "markup/no-invalid-nesting",
        &component("    <p>\n      <div>Body</div>\n    </p>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 5);
    assert!(
        diagnostics[0].message.contains("hydration mismatch"),
        "{diagnostics:?}"
    );
}

#[test]
fn invalid_nesting_sees_through_a_wrapper_and_an_expression() {
    // The `<p>` is closed wherever the `<div>` ends up, and a `{ … }` renders
    // what comes out of it where it was written.
    for markup in [
        "    <p><span><div>Body</div></span></p>",
        "    <p>{rows.map((row) => <ul key={row}>{row}</ul>)}</p>",
    ] {
        let diagnostics = lint_js("markup/no-invalid-nesting", &component(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn invalid_nesting_reports_the_other_rewrites_the_parser_makes() {
    for markup in [
        "    <a href=\"/a\"><a href=\"/b\">b</a></a>",
        "    <button><button>b</button></button>",
        "    <form><form><input /></form></form>",
        "    <ul><li>one<li>two</li></li></ul>",
        "    <table><div>cell</div></table>",
        "    <table><tr><td>cell</td></tr></table>",
        "    <tbody><td>cell</td></tbody>",
    ] {
        let diagnostics = lint_js("markup/no-invalid-nesting", &component(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn invalid_nesting_stops_at_a_component() {
    // `<Card>` may render its children anywhere, or not at all. Reporting the
    // `<div>` would be reporting a file this module cannot see.
    let diagnostics = lint_js(
        "markup/no-invalid-nesting",
        &component("    <p><Card><div>Body</div></Card></p>"),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn invalid_nesting_does_not_treat_an_attribute_value_as_a_child() {
    // The `<div>` is handed to `<p>` as a prop. Where it renders is `Panel`'s
    // decision, and it is certainly not inside this `<p>`'s markup.
    let diagnostics = lint_js(
        "markup/no-invalid-nesting",
        &component("    <Panel footer={<div>Body</div>}><p>Text</p></Panel>"),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn invalid_nesting_reports_one_break_once() {
    // The `<div>` closes the `<p>`, so the `<ul>` below it is not in a
    // paragraph any more. One broken structure, one diagnostic.
    let diagnostics = lint_js(
        "markup/no-invalid-nesting",
        &component("    <p><div><ul><li>one</li></ul></div></p>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn invalid_nesting_accepts_markup_a_browser_keeps() {
    for markup in [
        "    <p><span>text</span> and <em>more</em></p>",
        "    <div><p>one</p><p>two</p></div>",
        "    <ul><li>one</li><li>two</li></ul>",
        // A nested list is nested through a `<ul>`, which is legal.
        "    <ul><li>one<ul><li>two</li></ul></li></ul>",
        "    <table><tbody><tr><td>cell</td></tr></tbody></table>",
        "    <table><caption>c</caption><colgroup><col /></colgroup><tbody><tr><th>h</th></tr></tbody></table>",
        // An `<a>` inside a `<button>` is invalid ARIA practice and is not a
        // rewrite: the parser leaves it where it is, so this rule is silent.
        "    <button><a href=\"/x\">x</a></button>",
        "    <ul>{items.map((item) => <li key={item}>{item}</li>)}</ul>",
    ] {
        let diagnostics = lint_js("markup/no-invalid-nesting", &component(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- vite/hot-needs-optional-chaining ---------------------------------------

#[test]
fn hot_optional_chaining_reports_the_unguarded_form() {
    let diagnostics = lint_js(
        "vite/hot-needs-optional-chaining",
        "// @flow\nimport.meta.hot.accept((module) => module);\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!((diagnostics[0].line, diagnostics[0].column), (2, 1));
    assert!(
        diagnostics[0].message.contains("does not refine it"),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[0].message.contains("import.meta.hot.accept"),
        "{diagnostics:?}"
    );
}

#[test]
fn hot_optional_chaining_reports_the_guard_that_does_not_guard() {
    // The whole of ubugeeei-prod/uf#320: this is what every Vite guide writes,
    // it is what `uf check` rejects, and nothing told the reader why.
    let diagnostics = lint_js(
        "vite/hot-needs-optional-chaining",
        "// @flow\nif (import.meta.hot) {\n  import.meta.hot.accept();\n}\n",
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 3);
}

#[test]
fn hot_optional_chaining_accepts_the_two_forms_that_work() {
    for source in [
        // Optional chaining needs no refinement. `crates/uf_check/tests/
        // fixtures/vite_client.js` is written this way.
        "// @flow\nimport.meta.hot?.accept((module) => module);\n",
        "// @flow\nimport.meta.hot?.dispose((data) => data);\n",
        // A local identifier has a refinement key, so the guard works.
        "// @flow\nconst hot = import.meta.hot;\nif (hot) {\n  hot.accept();\n}\n",
        // Testing the value itself refines nothing and needs nothing:
        // `packages/router/client.js` gates a dynamic import on exactly this.
        "// @flow\nif (import.meta.hot != null) {\n  await import(\"./overlay.js\");\n}\n",
        // Other `import.meta` properties are always defined.
        "// @flow\nconst here = import.meta.url.toString();\n",
        "// @flow\nconst mode = import.meta.env.MODE;\n",
        // Source being *printed*, not run: `packages/vite/internal/refresh.js`
        // builds the HMR preamble as a template literal.
        "// @flow\nconst code = `if (import.meta.hot) { import.meta.hot.accept(); }`;\nexport { code };\n",
        // A property named `hot` on something else is not this at all.
        "// @flow\nconst options = { hot: {} };\noptions.hot.accept();\n",
    ] {
        let diagnostics = lint_js("vite/hot-needs-optional-chaining", source);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
    }
}

// --- the runner itself ------------------------------------------------------

#[test]
fn a_module_that_does_not_parse_is_left_to_flow_syntax() {
    // The tree a recovering parser produces is its guess at what was meant,
    // and reporting an `<img>` it invented would be reporting a file that does
    // not exist.
    let diagnostics = lint_js(
        "a11y/alt-text",
        "// @flow\ncomponent Page() renders React.Node {\n  return <img src=;\n}\n",
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_suppression_comment_covers_a_tree_rule() {
    let mut config = only("a11y/alt-text");
    config.lint.rules.insert(
        CompactString::const_new("uniflowed/unknown-lint-suppression"),
        RuleLevel::Error,
    );
    let source = "// @flow\ncomponent Page() renders React.Node {\n  \
                  // uf-lint-disable-next-line a11y/alt-text\n  \
                  return <img src=\"/cat.png\" />;\n}\n";

    let report = lint_source(&at("app/index.js", source), &config).expect("lint");

    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
}

#[test]
fn the_tree_rules_are_off_when_the_config_says_so() {
    let mut config = UniflowedConfig::default();
    for rule in [
        "a11y/alt-text",
        "a11y/aria-props",
        "a11y/heading-order",
        "a11y/label-has-associated-control",
        "a11y/no-static-element-interactions",
        "markup/no-invalid-nesting",
        "vite/hot-needs-optional-chaining",
    ] {
        config
            .lint
            .rules
            .insert(CompactString::from(rule), RuleLevel::Off);
    }
    let source = component("    <p><label>Email</label><img src=\"/x.png\" /></p>");

    let report = lint_source(&at("app/index.js", &source), &config).expect("lint");

    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule.starts_with("a11y/")
                || diagnostic.rule.starts_with("markup/")),
        "{:?}",
        report.diagnostics
    );
}
