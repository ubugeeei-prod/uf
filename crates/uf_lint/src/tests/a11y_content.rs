//! The `a11y/*` rules that ask whether an element has something to say.
//!
//! Each rule is tested first with the examples eslint-plugin-jsx-a11y
//! documents for it, adapted to a Flow component, and then with the shapes uf
//! answers differently — which are marked, with the reason, where they appear.

use super::tree::component;
use super::*;

/// Each markup in `cases` draws exactly one finding from `rule`.
#[track_caller]
pub(super) fn reports(rule: &str, cases: &[&str]) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    for markup in cases {
        let diagnostics = lint_js(rule, &component(markup));
        assert_eq!(
            diagnostics.len(),
            1,
            "{rule} should report `{markup}`: {diagnostics:?}"
        );
        found.extend(diagnostics);
    }
    found
}

/// No markup in `cases` draws a finding from `rule`.
#[track_caller]
pub(super) fn accepts(rule: &str, cases: &[&str]) {
    for markup in cases {
        let diagnostics = lint_js(rule, &component(markup));
        assert!(
            diagnostics.is_empty(),
            "{rule} should accept `{markup}`: {diagnostics:?}"
        );
    }
}

// --- a11y/anchor-has-content ------------------------------------------------

#[test]
fn anchor_has_content_reports_the_documented_failures() {
    reports(
        "a11y/anchor-has-content",
        &["<a />", "<a><TextWrapper aria-hidden /></a>"],
    );
}

#[test]
fn anchor_has_content_accepts_the_documented_passes() {
    accepts(
        "a11y/anchor-has-content",
        &[
            "<a>Anchor Content!</a>",
            "<a><TextWrapper /></a>",
            "<a dangerouslySetInnerHTML={{ __html: 'foo' }} />",
            "<a title='foo' />",
            "<a aria-label='foo' />",
        ],
    );
}

#[test]
fn anchor_has_content_looks_inside_host_elements() {
    // The plugin counts any child element as content. These are the links
    // with no name that lets through: an icon with no title, an image marked
    // decorative, an icon font, and a glyph hidden from the reader.
    let found = reports(
        "a11y/anchor-has-content",
        &[
            r#"<a href="/"><svg viewBox="0 0 24 24"><path d="M3 12l9-9 9 9" /></svg></a>"#,
            r#"<a href="/"><img src="/logo.svg" alt="" /></a>"#,
            r#"<a href="/"><i className="icon-home" /></a>"#,
            r#"<a href="/"><span aria-hidden="true">→</span></a>"#,
        ],
    );
    assert!(
        found
            .iter()
            .all(|diagnostic| diagnostic.message.contains("nothing inside this `<a>`")),
        "{found:?}"
    );
}

#[test]
fn anchor_has_content_reports_text_that_renders_nothing() {
    reports(
        "a11y/anchor-has-content",
        &[
            r#"<a href="/"> </a>"#,
            r#"<a href="/">{""}</a>"#,
            r#"<a href="/">{null}</a>"#,
            r#"<a href="/" aria-label="" />"#,
        ],
    );
}

#[test]
fn anchor_has_content_accepts_names_it_cannot_see_or_that_come_from_elsewhere() {
    accepts(
        "a11y/anchor-has-content",
        &[
            r#"<a href="/">{label}</a>"#,
            r#"<a href="/">{0}</a>"#,
            r#"<a href="/"><Icon name="home" /></a>"#,
            r#"<a href="/" {...props} />"#,
            r#"<a href="/" aria-labelledby="home-label"><svg /></a>"#,
            r#"<a href="/"><svg><title>Home</title><path d="M3 12l9-9 9 9" /></svg></a>"#,
            r#"<a href="/"><img src="/logo.svg" alt="Home" /></a>"#,
            r#"<a href="/"><span className="visually-hidden">Home</span><svg /></a>"#,
            r#"<a href="/"><span aria-hidden={hidden}>Home</span></a>"#,
            // A missing `alt` is `a11y/alt-text`'s finding, not a second one.
            r#"<a href="/"><img src="/logo.svg" /></a>"#,
            // A link hidden from assistive technology is announced by nobody.
            r#"<a href="/" aria-hidden="true" tabIndex={-1} />"#,
        ],
    );
}

#[test]
fn anchor_has_content_says_what_to_add_and_where() {
    let found = reports("a11y/anchor-has-content", &["<a />"]);

    assert_eq!(found[0].line, 4);
    assert!(found[0].message.contains("is empty"), "{found:?}");
    assert!(found[0].message.contains("aria-label"), "{found:?}");
}

// --- a11y/heading-has-content -----------------------------------------------

#[test]
fn heading_has_content_reports_the_documented_failures() {
    reports(
        "a11y/heading-has-content",
        &["<h1 />", "<h1><TextWrapper aria-hidden /></h1>"],
    );
}

#[test]
fn heading_has_content_accepts_the_documented_passes() {
    accepts(
        "a11y/heading-has-content",
        &[
            "<h1>Heading Content!</h1>",
            "<h1><TextWrapper /></h1>",
            "<h1 dangerouslySetInnerHTML={{ __html: 'foo' }} />",
        ],
    );
}

#[test]
fn heading_has_content_reports_every_level_and_content_nobody_hears() {
    reports(
        "a11y/heading-has-content",
        &[
            "<h2></h2>",
            r#"<h3>{" "}</h3>"#,
            r#"<h4><span aria-hidden="true">§</span></h4>"#,
            "<h6><br /></h6>",
        ],
    );
}

#[test]
fn heading_has_content_accepts_what_it_cannot_see_and_what_is_not_a_heading() {
    accepts(
        "a11y/heading-has-content",
        &[
            "<h2>{title}</h2>",
            "<h2 {...props} />",
            r#"<h2 aria-label="Settings" />"#,
            r#"<h2><Trans id="settings" /></h2>"#,
            "<header />",
            "<hgroup />",
            "<Heading />",
        ],
    );
}

// --- a11y/iframe-has-title --------------------------------------------------

#[test]
fn iframe_has_title_reports_the_documented_failures() {
    reports(
        "a11y/iframe-has-title",
        &[
            "<iframe />",
            r#"<iframe title="" />"#,
            "<iframe title={''} />",
            "<iframe title={``} />",
            "<iframe title={undefined} />",
            "<iframe title={false} />",
            "<iframe title={true} />",
            "<iframe title={42} />",
        ],
    );
}

#[test]
fn iframe_has_title_accepts_the_documented_passes() {
    accepts(
        "a11y/iframe-has-title",
        &[
            r#"<iframe title="This is a unique title" />"#,
            "<iframe title={uniqueTitle} />",
        ],
    );
}

#[test]
fn iframe_has_title_leaves_a_spread_and_a_hidden_frame_alone() {
    // The plugin reports `<iframe {...props} />`. A wrapper that spreads its
    // props onto a frame is where a title is passed through, and this rule
    // cannot see one arrive.
    accepts(
        "a11y/iframe-has-title",
        &[
            "<iframe {...props} />",
            r#"<iframe src="/pixel" aria-hidden="true" />"#,
            "<iframe title={`Map of ${venue}`} />",
            "<Frame />",
        ],
    );
}

#[test]
fn iframe_has_title_reports_a_blank_title_as_one_with_no_words() {
    let found = reports(
        "a11y/iframe-has-title",
        &[r#"<iframe src="/map" title="   " />"#],
    );

    assert!(found[0].message.contains("no words"), "{found:?}");
}

// --- a11y/html-has-lang -----------------------------------------------------

#[test]
fn html_has_lang_reports_the_documented_failure() {
    reports("a11y/html-has-lang", &["<html></html>"]);
}

#[test]
fn html_has_lang_accepts_the_documented_passes() {
    accepts(
        "a11y/html-has-lang",
        &[
            r#"<html lang="en"></html>"#,
            r#"<html lang="en-US"></html>"#,
            "<html lang={language}></html>",
        ],
    );
}

#[test]
fn html_has_lang_reports_a_lang_that_names_nothing() {
    reports(
        "a11y/html-has-lang",
        &[r#"<html lang=""></html>"#, "<html lang={undefined}></html>"],
    );
}

#[test]
fn html_has_lang_leaves_a_spread_and_components_alone() {
    accepts(
        "a11y/html-has-lang",
        &["<html {...props}></html>", "<Html />"],
    );
}

// --- a11y/img-redundant-alt -------------------------------------------------

#[test]
fn img_redundant_alt_reports_the_documented_failures() {
    let found = reports(
        "a11y/img-redundant-alt",
        &[
            r#"<img src="foo" alt="Photo of foo being weird." />"#,
            r#"<img src="bar" alt="Image of me at a bar!" />"#,
            r#"<img src="baz" alt="Picture of baz fixing a bug." />"#,
        ],
    );

    assert!(found[0].message.contains("\"photo\""), "{found:?}");
}

#[test]
fn img_redundant_alt_accepts_the_documented_passes() {
    accepts(
        "a11y/img-redundant-alt",
        &[
            r#"<img src="foo" alt="Foo eating a sandwich." />"#,
            r#"<img src="bar" aria-hidden alt="Picture of me taking a photo of an image" />"#,
            "<img src=\"baz\" alt={`Baz taking a ${photo}`} />",
        ],
    );
}

#[test]
fn img_redundant_alt_reports_the_medium_in_every_spelling() {
    reports(
        "a11y/img-redundant-alt",
        &[
            r#"<img src="team" alt="A photo of the team at the offsite" />"#,
            r#"<img src="ada" alt={"Image: Ada at the summit"} />"#,
            "<img src=\"ada\" alt={`Picture of ${name}`} />",
            r#"<img src="logo" alt="image" />"#,
            r#"<img src="launch" alt="Photos of the launch" />"#,
        ],
    );
}

#[test]
fn img_redundant_alt_accepts_a_photo_that_is_the_subject() {
    // The plugin reports the word anywhere. In "Ada taking a photo" the photo
    // is what the image shows, and nothing is said twice.
    accepts(
        "a11y/img-redundant-alt",
        &[
            r#"<img src="ada" alt="Ada taking a photo" />"#,
            r#"<img src="coast" alt="Satellite imagery of the coast" />"#,
            // A spread after `alt` may replace it.
            r#"<img src="ada" alt="Photo of Ada" {...props} />"#,
            "<img src=\"ada\" alt={caption} />",
            r#"<Image alt="Photo of Ada" />"#,
        ],
    );
}

// --- a11y/media-has-caption -------------------------------------------------

#[test]
fn media_has_caption_reports_media_with_no_captions() {
    // The plugin's documented failures are `<audio {...props} />` and
    // `<video {...props} />`; the next test says why a spread is left alone.
    // These are the same defect with nothing hidden in a spread.
    reports(
        "a11y/media-has-caption",
        &[
            r#"<audio src="/talk.mp3" />"#,
            r#"<video src="/talk.mp4"></video>"#,
            r#"<video src="/talk.mp4"><track kind="subtitles" src="/es.vtt" /></video>"#,
            // A spread written before `kind` cannot replace it.
            r#"<video src="/talk.mp4"><track {...props} kind="subtitles" /></video>"#,
            r#"<video src="/talk.mp4" muted={false} />"#,
            r#"<video><source src="/talk.mp4" type="video/mp4" /></video>"#,
        ],
    );
}

#[test]
fn media_has_caption_accepts_the_documented_passes() {
    accepts(
        "a11y/media-has-caption",
        &[
            r#"<audio><track kind="captions" {...props} /></audio>"#,
            r#"<video><track kind="captions" {...props} /></video>"#,
            "<video muted {...props} ></video>",
        ],
    );
}

#[test]
fn media_has_caption_leaves_what_it_cannot_see_alone() {
    // A spread may carry `muted` or the children, a component or an expression
    // child may render the `<track>`, and an expression `kind` may be captions.
    accepts(
        "a11y/media-has-caption",
        &[
            "<audio {...props} />",
            "<video {...props} />",
            r#"<video src="/loop.mp4" muted autoPlay loop />"#,
            r#"<video src="/talk.mp4" muted={muted} />"#,
            r#"<video src="/talk.mp4"><track kind="Captions" src="/en.vtt" /></video>"#,
            r#"<video src="/talk.mp4">{tracks}</video>"#,
            r#"<video src="/talk.mp4"><Captions src="/en.vtt" /></video>"#,
            r#"<video src="/talk.mp4"><track kind={kind} src="/en.vtt" /></video>"#,
            // A spread written after `kind` may replace it with "captions".
            r#"<video src="/talk.mp4"><track kind="subtitles" {...props} /></video>"#,
            r#"<Video src="/talk.mp4" />"#,
        ],
    );
}

// --- `undefined`, when the module gives the name to something ---------------

/// `a11y/anchor-is-valid` over `source` finds exactly the `href="#"` on `line`,
/// which is also the proof that the module parsed and the rule ran.
#[track_caller]
fn only_the_dead_href_on(line: usize, source: &str) {
    let diagnostics = lint_js("a11y/anchor-is-valid", source);
    assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
    assert_eq!(diagnostics[0].line, line, "{source}\n{diagnostics:?}");
}

#[test]
fn a_shadowed_undefined_is_not_read_as_nothing() {
    // `href={undefined}` renders no `href` only while `undefined` is the
    // global. Each module here gives the name to something else — a
    // parameter, an import, a `const` declared after the link that reads it —
    // so the link goes wherever that binding says.
    only_the_dead_href_on(
        5,
        r##"// @flow
component Nav(undefined: string) renders React.Node {
  return (
    <nav>
      <a href="#">Top</a>
      <a href={undefined}>Docs</a>
    </nav>
  );
}
"##,
    );
    only_the_dead_href_on(
        7,
        r##"// @flow
import undefined from './docs-url';

component Nav() renders React.Node {
  return (
    <nav>
      <a href="#">Top</a>
      <a href={undefined}>Docs</a>
    </nav>
  );
}
"##,
    );
    only_the_dead_href_on(
        5,
        r##"// @flow
component Nav() renders React.Node {
  return (
    <nav>
      <a href="#">Top</a>
      <a href={undefined}>Docs</a>
    </nav>
  );
}

const undefined = '/docs';
"##,
    );
}

#[test]
fn an_undefined_that_is_not_a_binding_still_reads_as_nothing() {
    // The word in a comment, as a default value and on the right of an
    // assignment declares nothing, so `undefined` is still the global.
    let diagnostics = lint_js(
        "a11y/anchor-is-valid",
        r#"// @flow
// `undefined` is the global in this module.
component Nav(fallback?: string) renders React.Node {
  let target = undefined;
  target = fallback ?? undefined;
  return <a href={undefined}>Docs</a>;
}
"#,
    );

    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 6, "{diagnostics:?}");
    assert!(
        diagnostics[0].message.contains("has no `href`"),
        "{diagnostics:?}"
    );
}

// --- a11y/anchor-ambiguous-text ---------------------------------------------

#[test]
fn anchor_ambiguous_text_reports_the_documented_failures() {
    reports(
        "a11y/anchor-ambiguous-text",
        &[
            "<a>here</a>",
            "<a>HERE</a>",
            "<a>link</a>",
            "<a>click here</a>",
            "<a>learn more</a>",
            "<a>learn more.</a>",
            "<a>learn more,</a>",
            "<a>learn more?</a>",
            "<a>learn more!</a>",
            "<a>learn more:</a>",
            "<a>learn more;</a>",
            "<a>a link</a>",
            "<a> a link </a>",
            "<a><span> click </span> here</a>",
            "<a>a<i></i> link</a>",
            "<a><i></i>a link</a>",
            r#"<a><span aria-hidden="true">more text</span>learn more</a>"#,
            r#"<a aria-label="click here">something</a>"#,
            r#"<a><img alt="click here"/></a>"#,
            r#"<a alt="tutorial on using eslint-plugin-jsx-a11y">click here</a>"#,
        ],
    );
}

#[test]
fn anchor_ambiguous_text_accepts_the_documented_passes() {
    accepts(
        "a11y/anchor-ambiguous-text",
        &[
            "<a>read this tutorial</a>",
            "<a>${here}</a>",
            r#"<a aria-label="tutorial on using eslint-plugin-jsx-a11y">click here</a>"#,
        ],
    );
}

#[test]
fn anchor_ambiguous_text_reports_read_more_and_content_past_an_empty_label() {
    // "read more" and "more" fail for the reason the plugin's phrases do, and
    // an empty `aria-label` is skipped by the name computation, so the text is
    // what is announced.
    reports(
        "a11y/anchor-ambiguous-text",
        &[
            r#"<a href="/pricing">Read more…</a>"#,
            r#"<a href="/pricing">More</a>"#,
            r#"<a href="/pricing" aria-label="">click here</a>"#,
            r#"<a href="/pricing">{"learn more"}</a>"#,
        ],
    );
}

#[test]
fn anchor_ambiguous_text_leaves_names_it_cannot_read_alone() {
    accepts(
        "a11y/anchor-ambiguous-text",
        &[
            r#"<a href="/docs">{label}</a>"#,
            r#"<a href="/docs"><Trans>click here</Trans></a>"#,
            r#"<a href="/docs" aria-labelledby="docs-heading">click here</a>"#,
            r#"<a href="/docs" aria-label={label}>click here</a>"#,
            "<a {...props}>here</a>",
            r#"<a href="/docs">read the migration guide</a>"#,
        ],
    );
}

#[test]
fn anchor_ambiguous_text_quotes_the_phrase_it_heard() {
    let found = reports("a11y/anchor-ambiguous-text", &["<a>Click Here!</a>"]);

    assert!(found[0].message.contains("\"click here\""), "{found:?}");
}

// --- a11y/anchor-is-valid ---------------------------------------------------

#[test]
fn anchor_is_valid_reports_the_documented_buttons_written_as_links() {
    let found = reports(
        "a11y/anchor-is-valid",
        &[
            "<a onClick={foo} />",
            r##"<a href="#" onClick={foo} />"##,
            r##"<a href={"#"} onClick={foo} />"##,
            "<a href={`#`} onClick={foo} />",
            r#"<a href="javascript:void(0)" onClick={foo} />"#,
            r#"<a href={"javascript:void(0)"} onClick={foo} />"#,
            "<a href={`javascript:void(0)`} onClick={foo} />",
        ],
    );

    assert!(
        found
            .iter()
            .all(|diagnostic| diagnostic.message.contains("<button type=\"button\">")),
        "{found:?}"
    );
}

#[test]
fn anchor_is_valid_reports_the_documented_missing_href() {
    let found = reports(
        "a11y/anchor-is-valid",
        &["<a />", "<a href={undefined} />", "<a href={null} />"],
    );

    assert!(
        found
            .iter()
            .all(|diagnostic| diagnostic.message.contains("has no `href`")),
        "{found:?}"
    );
}

#[test]
fn anchor_is_valid_reports_the_documented_invalid_href() {
    reports(
        "a11y/anchor-is-valid",
        &[
            r##"<a href="#" />"##,
            r##"<a href={"#"} />"##,
            "<a href={`#`} />",
            r#"<a href="javascript:void(0)" />"#,
            r#"<a href={"javascript:void(0)"} />"#,
            "<a href={`javascript:void(0)`} />",
        ],
    );
}

#[test]
fn anchor_is_valid_accepts_the_documented_passes() {
    accepts(
        "a11y/anchor-is-valid",
        &[
            r#"<a href="https://github.com" />"#,
            r##"<a href="#section" />"##,
            r#"<a href="foo" />"#,
            r#"<a href="/foo/bar" />"#,
            "<a href={someValidPath} />",
            r#"<a href="https://github.com" onClick={foo} />"#,
            r##"<a href="#section" onClick={foo} />"##,
            r#"<a href="foo" onClick={foo} />"#,
            r#"<a href="/foo/bar" onClick={foo} />"#,
            "<a href={someValidPath} onClick={foo} />",
        ],
    );
}

#[test]
fn anchor_is_valid_reads_a_url_the_way_the_url_parser_does() {
    // Leading spaces, letter case and a tab inside the scheme all still run
    // as `javascript:`.
    let found = reports(
        "a11y/anchor-is-valid",
        &[
            r#"<a href="" />"#,
            r#"<a href="  JavaScript:alert(1)" />"#,
            "<a href={\"java\\tscript:alert(1)\"} />",
            r##"<a href=" # " />"##,
        ],
    );

    assert!(found[0].message.contains("reloads"), "{found:?}");
    assert!(found[1].message.contains("React 19"), "{found:?}");
    assert!(found[2].message.contains("React 19"), "{found:?}");
}

#[test]
fn anchor_is_valid_leaves_what_it_cannot_see_alone() {
    accepts(
        "a11y/anchor-is-valid",
        &[
            "<a {...props} />",
            "<a {...props} onClick={foo} />",
            // An explicit role is a claim the role rules judge.
            r#"<a role="button" tabIndex={0} onClick={foo} onKeyDown={foo}>Save</a>"#,
            r##"<a href={cond ? "#" : url} onClick={foo} />"##,
            "<Link onClick={foo} />",
        ],
    );
}
