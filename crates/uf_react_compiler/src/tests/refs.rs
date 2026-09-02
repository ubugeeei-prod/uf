//! `react/no-ref-read-in-render`.

use super::{accepts, check, findings};
use crate::rule::Finding;

#[test]
fn reading_a_ref_during_render_is_rejected() {
    let diagnostics = check(
        "component Page() {\n  const box = useRef(null);\n  const width = box.current.offsetWidth;\n  return width;\n}\n",
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].finding, Finding::RefReadDuringRender);
    assert_eq!(diagnostics[0].line, 3);
    assert_eq!(diagnostics[0].symbol.as_deref(), Some("box"));
}

#[test]
fn reading_a_ref_in_the_returned_markup_is_rejected() {
    assert_eq!(
        findings(
            "component Page() {\n  const box = useRef(null);\n  return <p>{box.current}</p>;\n}\n"
        ),
        [Finding::RefReadDuringRender]
    );
}

#[test]
fn reading_a_ref_inside_a_condition_is_rejected() {
    assert_eq!(
        findings(
            "component Page(flag: boolean) {\n  if (flag) {\n    log(box.current);\n  }\n  const box = useRef(null);\n  return null;\n}\n"
        )
        .len(),
        0,
        "the binding is not a ref until it is declared"
    );

    assert_eq!(
        findings(
            "component Page(flag: boolean) {\n  const box = useRef(null);\n  if (flag) {\n    log(box.current);\n  }\n  return null;\n}\n"
        ),
        [Finding::RefReadDuringRender]
    );
}

#[test]
fn reading_a_ref_in_an_effect_is_accepted() {
    accepts(
        "component Page() {\n  const box = useRef(null);\n  useEffect(() => {\n    measure(box.current);\n  });\n  return null;\n}\n",
    );
}

#[test]
fn reading_a_ref_in_an_event_handler_is_accepted() {
    accepts(
        "component Page() {\n  const box = useRef(null);\n  const onClick = () => measure(box.current);\n  return onClick;\n}\n",
    );
}

#[test]
fn passing_a_ref_without_reading_it_is_accepted() {
    accepts("component Page() {\n  const box = useRef(null);\n  return <div ref={box} />;\n}\n");
}

#[test]
fn reading_another_property_of_a_ref_is_accepted() {
    accepts("component Page() {\n  const box = useRef(null);\n  return <div key={box.id} />;\n}\n");
}

#[test]
fn a_current_property_of_something_that_is_not_a_ref_is_accepted() {
    accepts("component Page(state: State) {\n  return state.current;\n}\n");
}

#[test]
fn a_destructured_ref_is_tracked() {
    // `const [box] = useRef(...)` is not idiomatic, but the binding is still
    // the ref object and reading it during render is still wrong.
    assert_eq!(
        findings("component Page() {\n  const [box] = useRef(null);\n  return box.current;\n}\n"),
        [Finding::RefReadDuringRender]
    );
}

#[test]
fn a_ref_read_in_a_custom_hook_is_rejected() {
    assert_eq!(
        findings(
            "hook useWidth(): number {\n  const box = useRef(null);\n  return box.current;\n}\n"
        ),
        [Finding::RefReadDuringRender]
    );
}

#[test]
fn a_ref_read_inside_a_nested_helper_is_accepted() {
    accepts(
        "component Page() {\n  const box = useRef(null);\n  function measure(): number {\n    return box.current;\n  }\n  return measure;\n}\n",
    );
}

#[test]
fn a_ref_reports_under_its_own_rule() {
    let diagnostics =
        check("component Page() {\n  const box = useRef(null);\n  return box.current;\n}\n");
    assert_eq!(diagnostics[0].rule(), "react/no-ref-read-in-render");
}
