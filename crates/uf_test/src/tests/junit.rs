//! The JUnit document, and the ways a test can break the XML around it.
//!
//! Every string in the document was written by whoever wrote the test — a
//! name, a matcher's message, a stack. A CI system that cannot parse the file
//! reports the run as a broken artefact rather than as a failure, which is a
//! worse outcome than no reporter at all, so what a test may put in a name is
//! the thing worth pinning.

use crate::plan::{SkipReason, TestPlan};
use crate::report::{
    AssertionFailure, FileReport, FileStatus, TestRecord, TestRunReport, TestStatus, TestSummary,
};
use crate::reporters::junit;

fn record(name: &str, status: TestStatus) -> TestRecord {
    TestRecord {
        file: String::from("src/a.test.js"),
        name: String::from(name),
        line: 3,
        column: 1,
        status,
        attempts: 1,
        duration_micros: 1_500,
        output: Vec::new(),
    }
}

fn report(files: Vec<FileReport>) -> TestRunReport {
    let mut summary = TestSummary {
        files: files.len(),
        duration_micros: 2_000_000,
        ..TestSummary::default()
    };
    for file in &files {
        if file.status.is_fatal() {
            summary.failed_files += 1;
        }
        for record in &file.records {
            match &record.status {
                TestStatus::Passed => summary.passed += 1,
                TestStatus::Failed { .. } => summary.failed += 1,
                TestStatus::Skipped { .. } => summary.skipped += 1,
                TestStatus::Todo => summary.todo += 1,
            }
        }
    }
    TestRunReport {
        plan: TestPlan::default(),
        files,
        summary,
    }
}

#[test]
fn one_testsuite_per_file_and_one_testcase_per_declaration() {
    let xml = junit(&report(vec![FileReport {
        file: String::from("src/a.test.js"),
        status: FileStatus::Completed,
        duration_micros: 1_250_000,
        records: vec![
            record("math > adds", TestStatus::Passed),
            record(
                "math > fails",
                TestStatus::Failed {
                    failures: vec![AssertionFailure {
                        message: String::from("expected 1 to be 2"),
                        line: 4,
                        column: 12,
                        span: 1,
                        expected: Some(String::from("2")),
                        received: Some(String::from("1")),
                        stack: None,
                    }],
                },
            ),
            record(
                "math > off",
                TestStatus::Skipped {
                    reason: SkipReason::Explicit,
                },
            ),
            record("math > later", TestStatus::Todo),
        ],
        output: Vec::new(),
    }]));

    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
    assert!(xml.contains("tests=\"4\" failures=\"1\" errors=\"0\" skipped=\"2\" time=\"2.000\""));
    assert!(xml.contains("<testsuite name=\"src/a.test.js\" tests=\"4\""));
    assert!(xml.contains(
        "<testcase name=\"math &gt; adds\" classname=\"src/a.test.js\" time=\"0.002\"/>"
    ));
    assert!(xml.contains("<failure message=\"expected 1 to be 2\" type=\"AssertionError\">"));
    assert!(xml.contains("expected: 2"));
    assert!(xml.contains("<skipped message=\"skipped\"/>"));
    assert!(xml.contains("<skipped message=\"todo\"/>"));
    assert!(xml.trim_end().ends_with("</testsuites>"));
}

#[test]
fn a_file_that_could_not_load_is_an_error_rather_than_an_empty_suite() {
    let xml = junit(&report(vec![FileReport {
        file: String::from("src/broken.test.js"),
        status: FileStatus::LoadFailed {
            message: String::from("Error: boom"),
            stack: None,
        },
        duration_micros: 10_000,
        records: Vec::new(),
        output: Vec::new(),
    }]));

    // A suite with no cases in it reads as a pass. The file failed, so it gets
    // a case of its own carrying the reason — which is what makes a CI system
    // show a red mark rather than an empty box.
    assert!(xml.contains(
        "<testsuite name=\"src/broken.test.js\" tests=\"1\" failures=\"0\" errors=\"1\""
    ));
    assert!(xml.contains("<error message=\"failed to load: Error: boom\" type=\"FileError\"/>"));
}

#[test]
fn a_test_name_cannot_break_the_document_it_is_reported_in() {
    let xml = junit(&report(vec![FileReport {
        file: String::from("src/a.test.js"),
        status: FileStatus::Completed,
        duration_micros: 1,
        records: vec![record(
            "</testcase><injected a=\"b\"> & \u{1b}[31mred\u{1b}[0m \u{7}",
            TestStatus::Passed,
        )],
        output: Vec::new(),
    }]));

    // The five predefined entities are escaped, so a name cannot close a tag
    // or open one.
    assert!(xml.contains("&lt;/testcase&gt;&lt;injected a=&quot;b&quot;&gt; &amp; "));
    assert!(!xml.contains("<injected"));
    // Control characters are dropped rather than escaped: XML 1.0 has no way to
    // write most of them at all, so escaping them would produce a document no
    // parser accepts — a green run that CI reports as a broken artefact.
    assert!(!xml.contains('\u{1b}'));
    assert!(!xml.contains('\u{7}'));
    assert!(xml.contains("[31mred"));
}
