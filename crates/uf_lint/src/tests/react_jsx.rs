//! The JSX rules `eslint-plugin-react` users know: `react/jsx-key`,
//! `react/no-array-index-key`, `react/jsx-no-duplicate-props`,
//! `react/no-children-prop`, `react/void-dom-elements-no-children` and
//! `react/jsx-no-comment-textnodes`.
//!
//! Each rule is tested from both sides, and the second side is the longer one:
//! the examples the plugin's own documentation gives, written as Flow, and then
//! the shapes a uf project writes that must stay quiet.

use super::*;

const KEY: &str = "react/jsx-key";
const INDEX_KEY: &str = "react/no-array-index-key";
const DUPLICATE: &str = "react/jsx-no-duplicate-props";
const CHILDREN: &str = "react/no-children-prop";
const VOID: &str = "react/void-dom-elements-no-children";
const COMMENT: &str = "react/jsx-no-comment-textnodes";
const UNESCAPED: &str = "react/no-unescaped-entities";

/// A Flow module with React in scope and a list item type to map over.
fn module(body: &str) -> String {
    format!(
        "// @flow\nimport * as React from \"react\";\n\ntype Item = {{ id: string, label: string, parts: Array<Item> }};\n\n{body}"
    )
}

/// `markup` returned from a component. The markup's first line is line 8.
fn page(markup: &str) -> String {
    module(&format!(
        "component Page(items: Array<Item>, children: React.Node) {{\n  return (\n    {markup}\n  );\n}}\n"
    ))
}

// --- react/jsx-key -----------------------------------------------------------

#[test]
fn jsx_key_reports_every_element_of_an_array_literal_without_one() {
    let diagnostics = lint_js(KEY, &page("[<b />, <i />, <u />]"));

    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
    assert!(
        diagnostics.iter().all(|found| found.line == 8),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[0].message.contains("`key`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_reports_an_element_returned_from_map() {
    let diagnostics = lint_js(
        KEY,
        &page("<ul>{items.map((item) => <li>{item.label}</li>)}</ul>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("`<li>`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_reports_an_element_built_by_array_from() {
    let diagnostics = lint_js(
        KEY,
        &page("<ul>{Array.from(items, (item) => <li>{item.label}</li>)}</ul>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn jsx_key_reads_every_return_of_a_block_body() {
    let diagnostics = lint_js(
        KEY,
        &module(
            "component Page(items: Array<Item>) {\n  return items.map((item) => {\n    if (item.parts.length === 0) {\n      return <p>{item.label}</p>;\n    }\n    return item.id === \"\" ? <hr /> : <section>{item.label}</section>;\n  });\n}\n",
        ),
    );

    assert_eq!(diagnostics.len(), 3, "{diagnostics:?}");
}

#[test]
fn jsx_key_reports_a_fragment_in_a_list_and_says_how_to_key_one() {
    let diagnostics = lint_js(
        KEY,
        &page("<dl>{items.map((item) => <><dt>{item.id}</dt><dd>{item.label}</dd></>)}</dl>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("Fragment key="),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_reports_a_key_hidden_in_a_spread_object() {
    let diagnostics = lint_js(
        KEY,
        &page("<ul>{items.map((item) => <li {...{ key: item.id, title: item.label }} />)}</ul>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("spread"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_reports_the_same_literal_key_twice_in_one_array() {
    let diagnostics = lint_js(KEY, &page("[<b key=\"a\" />, <i key=\"a\" />]"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("twice"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_leaves_one_key_on_the_two_branches_of_a_slot_alone() {
    // A conditional builds one slot of the array, and only one of its branches
    // ever fills it, so the `key` the two share is one key.
    let diagnostics = lint_js(
        KEY,
        &page("[items[0].id === \"\" ? <b key=\"a\" /> : <i key=\"a\" />, <u key=\"b\" />]"),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn jsx_key_reports_a_key_that_a_later_slot_takes_again() {
    // The same conditional, and the slot after it takes the key both branches
    // carry: whichever branch renders, two elements answer to `"a"`.
    let diagnostics = lint_js(
        KEY,
        &page("[items[0].id === \"\" ? <b key=\"a\" /> : <i key=\"a\" />, <u key=\"a\" />]"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("twice"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn jsx_key_accepts_every_list_that_is_keyed_or_cannot_be_seen() {
    for markup in [
        "[<b key=\"first\" />, <i key=\"second\" />]",
        "<ul>{items.map((item) => <li key={item.id}>{item.label}</li>)}</ul>",
        "<ul>{Array.from(items, (item) => <li key={item.id}>{item.label}</li>)}</ul>",
        // Only the element the callback returns is a list item; what it holds
        // is not.
        "<ul>{items.map((item) => <li key={item.id}><span>{item.label}</span></li>)}</ul>",
        // A spread may be carrying the key, and this rule cannot see inside
        // one.
        "<ul>{items.map((item) => <Row {...item} />)}</ul>",
        // `React.Children.map` keys what it returns by itself.
        "<ul>{React.Children.map(children, (child) => <li>{child}</li>)}</ul>",
        // Arrays that are not lists of elements.
        "<p>{items.filter((item) => item.id !== \"\").length}</p>",
        "<p title={items.map((item) => item.label).join(\", \")} />",
        // An element outside a list needs no key, spread or not.
        "<Hello key={items[0].id} {...{ id: 1, caption: \"a\" }} />",
    ] {
        let diagnostics = lint_js(KEY, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

#[test]
fn jsx_key_checks_children_by_binding_not_spelling() {
    let local_children = lint_js(
        KEY,
        &module(
            "component Page(items: Array<Item>) {\n  const Children = items;\n  return <ul>{Children.map((item) => <li>{item.label}</li>)}</ul>;\n}\n",
        ),
    );

    assert_eq!(local_children.len(), 1, "{local_children:?}");

    let imported_alias = lint_js(
        KEY,
        &module(
            "import { Children as kids } from \"react\";\n\
             component Page(children: React.Node) {\n  return <ul>{kids.map(children, (child) => <li>{child}</li>)}</ul>;\n}\n",
        ),
    );

    assert!(imported_alias.is_empty(), "{imported_alias:?}");
}

// --- react/no-array-index-key -----------------------------------------------

#[test]
fn array_index_key_reports_an_index_used_as_a_key() {
    let diagnostics = lint_js(
        INDEX_KEY,
        &page("<ul>{items.map((item, index) => <li key={index}>{item.label}</li>)}</ul>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("`index`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn array_index_key_reports_every_way_an_index_is_spelled_into_a_key() {
    for key in [
        "{`item-${index}`}",
        "{index.toString()}",
        "{String(index)}",
        "{\"item-\" + index}",
        // A property of the item does not rescue the index: two items with
        // one label that swap places keep each other's keys.
        "{`${item.label}-${index}`}",
    ] {
        let markup =
            format!("<ul>{{items.map((item, index) => <li key={key}>{{item.label}}</li>)}}</ul>");
        let diagnostics = lint_js(INDEX_KEY, &page(&markup));
        assert_eq!(diagnostics.len(), 1, "{key}: {diagnostics:?}");
    }
}

#[test]
fn array_index_key_follows_the_index_through_every_iteration_method() {
    // `<Row />` rather than an empty `<li />`: a component may keep state for
    // the key to misplace, and an empty host element keeps none.
    for markup in [
        "<ul>{items.forEach((item, index) => { rows.push(<Row key={index} />); })}</ul>",
        "<ul>{items.filter((item, index) => { rows.push(<Row key={index} />); return true; })}</ul>",
        "<ul>{items.flatMap((item, index) => [<Row key={index} />])}</ul>",
        "<ul>{items.reduce((rows, item, index) => rows.concat(<Row key={index} />), [])}</ul>",
        "<ul>{React.Children.map(children, (child, index) => React.cloneElement(child, { key: index }))}</ul>",
    ] {
        let diagnostics = lint_js(INDEX_KEY, &page(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn array_index_key_accepts_keys_that_are_not_positions() {
    for markup in [
        "<ul>{items.map((item) => <li key={item.id}>{item.label}</li>)}</ul>",
        "<ul>{items.map((item) => <li key={`item-${item.id}`}>{item.label}</li>)}</ul>",
        // The index may be used for anything but the key.
        "<ul>{items.map((item, index) => <li key={item.id} data-index={index}>{item.label}</li>)}</ul>",
        // The inner callback's first parameter is its item, whatever it is
        // called.
        "<ul>{items.map((item, index) => item.parts.map((index) => <li key={index} />))}</ul>",
        // A fixed-length list never reorders, and `Array.from` is how one is
        // built.
        "<p>{Array.from({ length: 5 }, (_, index) => <b key={index} />)}</p>",
        // A key that also holds the item changes when the item does, so React
        // remounts the element rather than handing it another item's state.
        // `docs/app/reference/ui/registry.js` keys its code spans this way.
        "<p>{items[0].label.split(\" \").map((word, index) => <b key={`${index}:${word}`}>{word}</b>)}</p>",
        "<p>{items[0].label.split(\" \").map((word, index) => <b key={String(index) + word}>{word}</b>)}</p>",
        // An empty cell keeps no state for a key to hand over.
        // `packages/ui/calendar.js` keys its blank days by column.
        "<tr>{items.map((item, column) => <td key={`blank-${column}`} />)}</tr>",
        "<ul>{React.Children.map(children, (child) => React.cloneElement(child, { key: child.key }))}</ul>",
    ] {
        let diagnostics = lint_js(INDEX_KEY, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

#[test]
fn array_index_key_leaves_a_prop_called_index_alone() {
    let diagnostics = lint_js(
        INDEX_KEY,
        &module("component Row(index: number) {\n  return <li key={index} />;\n}\n"),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn array_index_key_still_reports_an_empty_control() {
    // A control keeps something the element around it does not, so an index key
    // hands one item's to another even where the element holds nothing else: an
    // `<input>` keeps what was typed into it, and a `<button>` — the empty
    // element a row of dots or a grid of swatches is built from — keeps the
    // browser's focus.
    for markup in [
        "<form>{items.map((item, index) => <input key={index} name={item.id} />)}</form>",
        "<div>{items.map((item, index) => <button key={index} onClick={() => pick(item)} />)}</div>",
    ] {
        let diagnostics = lint_js(INDEX_KEY, &page(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

// --- react/jsx-no-duplicate-props -------------------------------------------

#[test]
fn duplicate_props_reports_the_second_of_two() {
    let diagnostics = lint_js(DUPLICATE, &page("<Hello name=\"John\" name=\"John\" />"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        (diagnostics[0].line, diagnostics[0].column),
        (8, 24),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[0].message.contains("`name`"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn duplicate_props_sees_through_a_spread_between_them() {
    let diagnostics = lint_js(
        DUPLICATE,
        &page("<Hello name=\"a\" {...items[0]} name=\"b\" />"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
}

#[test]
fn duplicate_props_accepts_distinct_props_and_a_case_that_differs() {
    for markup in [
        "<Hello firstname=\"John\" lastname=\"Doe\" />",
        "<Hello name=\"a\" Name=\"b\" />",
        "<input value=\"a\" {...items[0]} />",
    ] {
        let diagnostics = lint_js(DUPLICATE, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- react/no-children-prop -------------------------------------------------

#[test]
fn children_prop_reports_children_passed_as_a_prop() {
    for markup in [
        "<div children=\"Children\" />",
        "<MyComponent children={<AnotherComponent />} />",
        "<MyComponent children={[\"Child 1\", \"Child 2\"]} />",
        "React.createElement(\"div\", { children: \"Children\" })",
    ] {
        let diagnostics = lint_js(CHILDREN, &page(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn children_prop_says_when_nested_children_replace_the_prop() {
    let diagnostics = lint_js(
        CHILDREN,
        &page("<MyComponent children=\"lost\">kept</MyComponent>"),
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("never renders"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn children_prop_accepts_children_where_they_belong() {
    for markup in [
        "<div>Children</div>",
        "<MyComponent>Children</MyComponent>",
        "<MyComponent>\n      <span>Child 1</span>\n      <span>Child 2</span>\n    </MyComponent>",
        "React.createElement(\"div\", {}, \"Children\")",
        "React.createElement(\"div\", null, \"Child 1\", \"Child 2\")",
    ] {
        let diagnostics = lint_js(CHILDREN, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

#[test]
fn children_prop_reads_element_factories_by_binding_not_spelling() {
    let alias = lint_js(
        CHILDREN,
        "// @flow\nimport { createElement as h } from \"react\";\nconst view = h(\"div\", { children: \"Children\" });\n",
    );

    assert_eq!(alias.len(), 1, "{alias:?}");

    let namespace = lint_js(
        CHILDREN,
        "// @flow\nimport * as R from \"react\";\nconst view = R.createElement(\"div\", { children: \"Children\" });\n",
    );

    assert_eq!(namespace.len(), 1, "{namespace:?}");

    let parameter_shadow = lint_js(
        CHILDREN,
        "// @flow\nimport { createElement } from \"react\";\nfunction render(createElement) {\n  return createElement(\"div\", { children: \"Children\" });\n}\n",
    );

    assert!(parameter_shadow.is_empty(), "{parameter_shadow:?}");

    let commonjs = lint_js(
        CHILDREN,
        "// @flow\nconst { createElement: h } = require(\"react\");\nconst view = h(\"div\", { children: \"Children\" });\n",
    );

    assert_eq!(commonjs.len(), 1, "{commonjs:?}");
}

// --- react/void-dom-elements-no-children ------------------------------------

#[test]
fn void_elements_report_children_in_every_form() {
    for markup in [
        "<br>Children</br>",
        "<br children=\"Children\" />",
        "<br dangerouslySetInnerHTML={{ __html: \"HTML\" }} />",
        "<img src=\"/a.png\" alt=\"\"><b /></img>",
        "React.createElement(\"br\", undefined, \"Children\")",
        "React.createElement(\"br\", { children: \"Children\" })",
        "React.createElement(\"br\", { dangerouslySetInnerHTML: { __html: \"HTML\" } })",
    ] {
        let diagnostics = lint_js(VOID, &page(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
        assert!(
            diagnostics[0].message.contains("throws"),
            "{}",
            diagnostics[0].message
        );
    }
}

#[test]
fn void_elements_accept_everything_that_is_not_a_void_element_with_content() {
    for markup in [
        "<div>Children</div>",
        "<div children=\"Children\" />",
        "<div dangerouslySetInnerHTML={{ __html: \"HTML\" }} />",
        "<br />",
        "<br></br>",
        // Whitespace that runs across a line break is not a child in JSX.
        "<br>\n    </br>",
        // A spread may carry children, and this rule cannot see inside one.
        "<img src=\"/a.png\" alt=\"\" {...items[0]} />",
        // A component that shares a name renders whatever it renders.
        "<Img>Children</Img>",
        "React.createElement(\"div\", undefined, \"Children\")",
        "React.createElement(\"br\", { className: \"gap\" })",
    ] {
        let diagnostics = lint_js(VOID, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- react/jsx-no-comment-textnodes -----------------------------------------

#[test]
fn comment_textnodes_report_a_line_comment_written_as_text() {
    let diagnostics = lint_js(COMMENT, &page("<div>// empty div</div>"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        (diagnostics[0].line, diagnostics[0].column),
        (8, 10),
        "{diagnostics:?}"
    );
    assert!(
        diagnostics[0].message.contains("{/*"),
        "{}",
        diagnostics[0].message
    );
}

#[test]
fn comment_textnodes_report_a_block_comment_on_its_own_line() {
    let diagnostics = lint_js(COMMENT, &page("<div>\n      /* empty div */\n    </div>"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(
        (diagnostics[0].line, diagnostics[0].column),
        (9, 7),
        "{diagnostics:?}"
    );
}

#[test]
fn comment_textnodes_accept_real_comments_and_slashes_that_are_not_one() {
    for markup in [
        "<div>{/* empty div */}</div>",
        "<div /* empty div */></div>",
        "<div className={\"foo\" /* temp class */}></div>",
        "<p>See https://example.com for more</p>",
        "<p>a // b</p>",
        // Text inside code is meant to be read, slashes and all.
        "<code>// a comment in a code sample</code>",
        "<pre>\n/* shown as written */\n</pre>",
    ] {
        let diagnostics = lint_js(COMMENT, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

// --- react/no-unescaped-entities ---------------------------------------------

#[test]
fn unescaped_entities_reports_the_two_that_are_worth_looking_at() {
    for markup in ["<p>a > b</p>", "<p>a lost } brace</p>"] {
        let diagnostics = lint_js(UNESCAPED, &page(markup));
        assert_eq!(diagnostics.len(), 1, "{markup}: {diagnostics:?}");
    }
}

#[test]
fn unescaped_entities_names_the_escape_to_write() {
    let diagnostics = lint_js(UNESCAPED, &page("<p>a > b</p>"));

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("&gt;"), "{diagnostics:?}");
}

#[test]
fn unescaped_entities_leaves_prose_and_escaped_text_alone() {
    for markup in [
        // Render exactly as written; reporting these makes `don't` a finding,
        // which is where uf answers less than the plugin.
        "<p>don't stop</p>",
        "<p>she said \"hi\"</p>",
        // Already escaped: the scan reads `raw`, where this is four characters.
        "<p>a &gt; b</p>",
        // Meant as written, like the comment rule leaves these alone.
        "<code>a > b</code>",
        "<pre>if (a > b)</pre>",
        // An expression container is not text.
        "<p>{value}</p>",
        "<p>plain words</p>",
    ] {
        let diagnostics = lint_js(UNESCAPED, &page(markup));
        assert!(diagnostics.is_empty(), "{markup}: {diagnostics:?}");
    }
}

/// `react/no-unescaped-entities` and the shipped
/// `react/jsx-no-comment-textnodes` read the same text children and ask
/// different questions of them, so the markup either reports is silent from the
/// other. A change that lets both answer one of these fails here.
#[test]
fn the_two_text_rules_divide_the_text_between_them() {
    let stray = "<p>a > b</p>";
    assert_eq!(lint_js(UNESCAPED, &page(stray)).len(), 1);
    assert!(lint_js(COMMENT, &page(stray)).is_empty());

    let commented = "<p>// a note</p>";
    assert_eq!(lint_js(COMMENT, &page(commented)).len(), 1);
    assert!(lint_js(UNESCAPED, &page(commented)).is_empty());
}

// --- shared -----------------------------------------------------------------

#[test]
fn a_module_that_does_not_parse_reports_nothing_here() {
    // `flow/syntax` owns the report; a recovered tree is the parser's guess.
    let diagnostics = lint_js(KEY, &page("[<b />, <i "));

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_rule_can_be_suppressed_on_its_line() {
    let diagnostics = lint_js(
        KEY,
        &module(
            "component Page(items: Array<Item>) {\n  // uf-lint-disable-next-line react/jsx-key\n  return items.map((item) => <li>{item.label}</li>);\n}\n",
        ),
    );

    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
