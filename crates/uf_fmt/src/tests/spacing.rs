//! Which adjacent tokens are separated by a space -- including the cases where
//! a wrong answer would glue two operators into a third one that means something
//! else entirely.

use super::*;

#[test]
fn normalizes_spacing_around_operators_and_separators() {
    similar_asserts::assert_eq!(format("const x=a+b*c;\n"), "const x = a + b * c;\n");
    similar_asserts::assert_eq!(format("f( a ,b );\n"), "f(a, b);\n");
    similar_asserts::assert_eq!(format("const a=[ 1 ,2 ];\n"), "const a = [1, 2];\n");
}

#[test]
fn keeps_unary_operators_attached_to_their_operand() {
    similar_asserts::assert_eq!(format("const x = - 1;\n"), "const x = -1;\n");
    similar_asserts::assert_eq!(format("const y = ! ok;\n"), "const y = !ok;\n");
    similar_asserts::assert_eq!(format("f(... args);\n"), "f(...args);\n");
    similar_asserts::assert_eq!(format("const z = a - -b;\n"), "const z = a - -b;\n");
}

#[test]
fn keeps_increments_attached_to_their_operand() {
    similar_asserts::assert_eq!(format("for (;;) i ++;\n"), "for (;;) i++;\n");
    similar_asserts::assert_eq!(format("++ i;\n"), "++i;\n");
}

#[test]
fn statement_headers_keep_a_space_before_the_parenthesis() {
    similar_asserts::assert_eq!(format("if(x){f(y);}\n"), "if (x) { f(y); }\n");
    similar_asserts::assert_eq!(format("while(x)f();\n"), "while (x) f();\n");
}

#[test]
fn for_headers_keep_their_semicolons() {
    similar_asserts::assert_eq!(
        format("for(let i=0;i<n;i++){}\n"),
        "for (let i = 0; i < n; i++) {}\n"
    );
    similar_asserts::assert_eq!(format("for( ; ; ){}\n"), "for (;;) {}\n");
}

#[test]
fn object_literals_keep_padded_braces_and_arrays_do_not() {
    similar_asserts::assert_eq!(format("const o={a:1};\n"), "const o = { a: 1 };\n");
    similar_asserts::assert_eq!(format("const o={};\n"), "const o = {};\n");
    similar_asserts::assert_eq!(format("const a=[1];\n"), "const a = [1];\n");
}

#[test]
fn member_access_and_calls_stay_tight() {
    similar_asserts::assert_eq!(format("a . b ( c ) [ 0 ];\n"), "a.b(c)[0];\n");
    similar_asserts::assert_eq!(format("a ?. b;\n"), "a?.b;\n");
}

#[test]
fn adjacent_operators_are_never_glued_into_a_different_token() {
    // Removing these spaces would turn `+ +` into `++` and change the program.
    for source in [
        "a + +b;\n",
        "a - -b;\n",
        "a + ++b;\n",
        "a - --b;\n",
        "a / /re/.source;\n",
        "a < <T>(x) => x;\n",
    ] {
        let output = format(source);
        assert_token_preserving(source, &output);
    }
}

#[test]
fn closing_type_arguments_may_be_joined_into_one_token() {
    // `Array<Array<T> >` legally becomes `Array<Array<T>>`; a Flow parser
    // splits the `>>` again when it closes type arguments.
    let output = format("type A = Array<Array<Array<T> > >;\n");
    similar_asserts::assert_eq!(output, "type A = Array<Array<Array<T>>>;\n");
    assert_token_preserving("type A = Array<Array<Array<T> > >;\n", &output);
}

#[test]
fn shift_operators_keep_their_spacing() {
    similar_asserts::assert_eq!(
        format("const x = a >> b >>> c << d;\n"),
        "const x = a >> b >>> c << d;\n"
    );
}

#[test]
fn ternaries_are_spaced_and_object_keys_are_not() {
    similar_asserts::assert_eq!(format("const x=a?b:c;\n"), "const x = a ? b : c;\n");
    similar_asserts::assert_eq!(
        format("const x={k:a?1:2,j:3};\n"),
        "const x = { k: a ? 1 : 2, j: 3 };\n"
    );
}

#[test]
fn continuation_lines_of_a_call_chain_are_indented() {
    similar_asserts::assert_eq!(
        format("promise\n.then(a)\n.catch(b);\n"),
        "promise\n  .then(a)\n  .catch(b);\n"
    );
}

#[test]
fn switch_cases_sit_one_level_inside_the_switch() {
    similar_asserts::assert_eq!(
        format("switch (x) {\ncase 1:\nf();\nbreak;\ndefault:\ng();\n}\n"),
        "switch (x) {\n  case 1:\n    f();\n    break;\n  default:\n    g();\n}\n"
    );
}

#[test]
fn generator_stars_hug_the_function_keyword() {
    similar_asserts::assert_eq!(
        format("function * gen() { yield * other(); }\n"),
        "function* gen() { yield* other(); }\n"
    );
}
