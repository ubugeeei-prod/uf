//! Scanner tests: what counts as code on a line, what carries into the next one,
//! and the byte offsets every diagnostic position is derived from.

use super::*;
use crate::SourceFile;

fn file(source: &str) -> SourceFile {
    SourceFile {
        path: "app/index.js".to_string(),
        source: source.to_string(),
    }
}

fn codes(source: &str) -> Vec<String> {
    let file = file(source);
    FileScan::new(&file)
        .lines
        .iter()
        .map(|line| line.code().to_string())
        .collect()
}

#[test]
fn line_comments_are_stripped_from_code() {
    assert_eq!(codes("let a = 1; // note\n"), vec!["let a = 1; ", ""]);
}

#[test]
fn urls_inside_strings_are_not_mistaken_for_comments() {
    assert_eq!(
        codes("const u = \"https://x.dev\"; // real\n"),
        vec!["const u = \"https://x.dev\"; ", ""]
    );
}

#[test]
fn block_comments_span_lines() {
    assert_eq!(
        codes("a;\n/* one\n two */ b;\nc;\n"),
        vec!["a;", "", " b;", "c;", ""]
    );
}

#[test]
fn unterminated_template_literals_carry_across_lines() {
    assert_eq!(
        codes("const t = `a\n// not a comment\n`;\n"),
        vec!["const t = `a", "// not a comment", "`;", ""]
    );
}

#[test]
fn escaped_quotes_do_not_end_a_string() {
    assert_eq!(
        codes("const s = 'a\\'// b'; c;\n"),
        vec!["const s = 'a\\'// b'; c;", ""]
    );
}

#[test]
fn crlf_terminators_are_trimmed_from_line_text() {
    let file = file("a;\r\nb;\r\n");
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines[0].text, "a;");
    assert_eq!(scan.lines[1].text, "b;");
}

#[test]
fn line_offsets_match_the_line_index() {
    let file = file("one\ntwo\nthree\n");
    let scan = FileScan::new(&file);
    for (position, line) in scan.lines.iter().enumerate() {
        assert_eq!(scan.index.line_col(line.offset).line, position + 1);
    }
}

#[test]
fn brace_depth_tracks_across_lines() {
    let file = file("component A() {\n  const x = { y: 1 };\n}\n");
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines[0].depth_at_start, 0);
    assert_eq!(scan.lines[1].depth_at_start, 1);
    assert_eq!(scan.lines[2].depth_at_start, 1);
    assert_eq!(scan.lines[3].depth_at_start, 0);
}

#[test]
fn first_code_line_skips_comments_and_blanks() {
    let file = file("\n// @flow\n\n'use client';\n");
    let scan = FileScan::new(&file);
    assert_eq!(scan.facts.first_code_line, Some(3));
}

#[test]
fn empty_input_has_one_empty_line() {
    let file = file("");
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines.len(), 1);
    assert_eq!(scan.facts.first_code_line, None);
}

#[test]
fn byte_order_mark_does_not_break_offsets() {
    let file = file("\u{feff}// @flow\nlet a = 1;\n");
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines[1].text, "let a = 1;");
    assert_eq!(scan.index.line_col(scan.lines[1].offset).line, 2);
}

#[test]
fn non_ascii_lines_keep_byte_offsets_consistent() {
    let file = file("const s = 'ünïcødé'; // ok\nlet a = 1;\n");
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines[1].text, "let a = 1;");
    assert_eq!(scan.index.line_col(scan.lines[1].offset).column, 1);
}

#[test]
fn find_words_respects_identifier_boundaries() {
    assert_eq!(
        find_words("any anything many any", "any").collect::<Vec<_>>(),
        vec![0, 18]
    );
    assert_eq!(
        find_words("$any", "any").collect::<Vec<_>>(),
        Vec::<usize>::new()
    );
}

#[test]
fn find_all_reports_every_occurrence() {
    assert_eq!(find_all("aXbXc", "X").collect::<Vec<_>>(), vec![1, 3]);
    assert_eq!(find_all("abc", "").collect::<Vec<_>>(), Vec::<usize>::new());
}

#[test]
fn identifier_len_rejects_digit_starts() {
    assert_eq!(identifier_len("useThing(", 0), 8);
    assert_eq!(identifier_len("9lives", 0), 0);
    assert_eq!(identifier_len("(", 0), 0);
}

#[test]
fn hook_names_need_an_uppercase_fourth_character() {
    assert!(is_hook_name("useState"));
    assert!(!is_hook_name("used"));
    assert!(!is_hook_name("use"));
    assert!(!is_hook_name("useful"));
}

#[test]
fn very_large_input_scans_without_quadratic_blowup() {
    let source = "let a = 1; // c\n".repeat(20_000);
    let file = file(&source);
    let scan = FileScan::new(&file);
    assert_eq!(scan.lines.len(), 20_001);
    assert_eq!(scan.lines[0].code(), "let a = 1; ");
}

#[test]
fn a_needle_inside_a_string_is_known_to_be_inside_one() {
    let code = r#"const message = "no globalThis.fetch here";"#;
    let at = code.find("globalThis.fetch").expect("the needle is there");

    assert!(in_string(code, at));
}

#[test]
fn a_needle_outside_every_string_is_not() {
    let code = r#"globalThis.fetch = mine;"#;

    assert!(!in_string(code, 0));
}

#[test]
fn a_closed_string_does_not_swallow_what_follows_it() {
    let code = r#"log("done"); globalThis.fetch = mine;"#;
    let at = code.find("globalThis").expect("the needle is there");

    assert!(!in_string(code, at));
}

#[test]
fn an_escaped_quote_does_not_close_the_string() {
    let code = r#"const s = "a \" globalThis.fetch";"#;
    let at = code.find("globalThis").expect("the needle is there");

    assert!(in_string(code, at));
}

#[test]
fn each_quote_style_opens_a_string() {
    for code in [
        "const s = 'globalThis.fetch';",
        "const s = \"globalThis.fetch\";",
        "const s = `globalThis.fetch`;",
    ] {
        let at = code.find("globalThis").expect("the needle is there");
        assert!(in_string(code, at), "{code}");
    }
}
