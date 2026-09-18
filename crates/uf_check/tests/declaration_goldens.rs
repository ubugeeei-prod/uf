//! Every `uf_dts` golden, checked by Flow itself.
//!
//! `uf_dts`'s own golden tests pin what the translation prints. They cannot
//! say whether Flow accepts it, and a translation Flow rejects is worse than
//! none: the declaration it was meant to type is `any` again, and the
//! diagnostic saying so is about a file nobody asked to check. So each case's
//! expectation — with the translations of the other files in its package
//! beside it — is checked here with the same checker `uf check` runs, and any
//! diagnostic at all fails the test.
//!
//! The consumer tests below are the other half: a translation that checks
//! clean because everything in it became `any` would pass the first test, so
//! each one writes a misuse against a translated declaration and asserts that
//! it is an error.

#![cfg(feature = "upstream-typecheck")]

use std::fs;
use std::path::{Path, PathBuf};

use uf_check::{CheckLimits, Source, TypeDiagnostic, check_sources};

fn goldens() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../uf_dts/tests/goldens")
}

fn cases() -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = fs::read_dir(goldens())
        .expect("uf_dts's goldens are beside this crate")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("expected.js.flow").is_file())
        .collect();
    cases.sort();
    assert!(
        !cases.is_empty(),
        "no goldens found in {}",
        goldens().display()
    );
    cases
}

/// The translated package of one case, as batch paths and Flow source, with
/// `input.d.ts` as its reviewed expectation rather than as translated now.
fn translated(case: &Path) -> Vec<(String, String)> {
    let name = case
        .file_name()
        .expect("a case directory")
        .to_string_lossy()
        .into_owned();
    let translation = uf_dts::translate(&["input.d.ts"], &mut |path| {
        fs::read_to_string(case.join(path)).ok()
    });
    let mut files = Vec::new();
    for module in translation.modules {
        let flow = if module.path == "input.d.ts" {
            fs::read_to_string(case.join("expected.js.flow")).expect("the expectation is readable")
        } else {
            match module.flow {
                Some(flow) => flow,
                None => continue,
            }
        };
        files.push((format!("{name}/{}", uf_dts::flow_path(&module.path)), flow));
    }
    // The other packages the cases import, as the smallest Flow packages that
    // export what they name. Without them each import is an unresolved
    // module, and every use of what it binds as a type is an error that says
    // nothing about the translation.
    for (path, source) in [
        (
            "node_modules/external-package/package.json",
            r#"{ "name": "external-package", "exports": { ".": "./index.js" } }"#,
        ),
        (
            "node_modules/external-package/index.js",
            "// @flow\ndeclare export class External {}\n",
        ),
        (
            "node_modules/some-package/package.json",
            r#"{ "name": "some-package", "exports": { ".": "./index.js" } }"#,
        ),
        (
            "node_modules/some-package/index.js",
            "// @flow\nexport interface Base { base: string }\n",
        ),
    ] {
        files.push((path.to_owned(), source.to_owned()));
    }
    files
}

fn check(files: &[(String, String)]) -> Vec<TypeDiagnostic> {
    let sources: Vec<Source<'_>> = files
        .iter()
        .map(|(path, source)| Source::new(path, source))
        .collect();
    check_sources(&sources, &[], &CheckLimits::default().without_timeout())
        .expect("the checker runs")
        .diagnostics
        .into_iter()
        // A manifest is in the batch to name its package, and the checker
        // also parses it as a program — which `uf check` drops, for the reason
        // `commands::check::type_check` gives.
        .filter(|diagnostic| !diagnostic.primary.path.ends_with("package.json"))
        // The translation gives a type parameter the variance TypeScript
        // measures, and TypeScript measures a method's parameters bivariantly
        // where Flow reads them contravariantly: `interface ZodType<out O>`
        // with a `check(fn: (value: O) => void): this` is a declaration Flow
        // reports. It reports it and then types every use of the interface
        // by the variance it was given — which is what
        // `a_schema_interface_and_its_constructor_are_both_typed` holds it to
        // — so this finding says nothing about whether the translation
        // types its uses.
        .filter(|diagnostic| diagnostic.code != Some("incompatible-variance"))
        .collect()
}

fn describe(diagnostic: &TypeDiagnostic) -> String {
    format!(
        "{}:{}: [{}] {}",
        diagnostic.primary.path,
        diagnostic.primary.start.line,
        diagnostic.code.unwrap_or("<none>"),
        diagnostic.message_text()
    )
}

#[test]
fn every_golden_expectation_is_flow_the_checker_accepts() {
    let mut failures = Vec::new();
    for case in cases() {
        failures.extend(check(&translated(&case)).iter().map(describe));
    }
    assert!(
        failures.is_empty(),
        "Flow rejected a translation:\n{}",
        failures.join("\n")
    );
}

/// The lines of `consumer` that have an error, when it is checked against the
/// translated package of `case`.
fn consumer_errors(case: &str, consumer: &str) -> Vec<u32> {
    let mut files = translated(&goldens().join(case));
    let path = format!("{case}/consumer.js");
    files.push((path.clone(), consumer.to_owned()));
    let diagnostics = check(&files);
    let mut lines: Vec<u32> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.primary.path == path && diagnostic.is_error())
        .map(|diagnostic| diagnostic.primary.start.line)
        .collect();
    lines.dedup();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.primary.path == path),
        "the translation itself has diagnostics:\n{}",
        diagnostics
            .iter()
            .map(describe)
            .collect::<Vec<_>>()
            .join("\n")
    );
    lines
}

#[test]
fn a_schema_interface_and_its_constructor_are_both_typed() {
    // zod's shape: `interface ZodString` beside `const ZodString`, and an
    // `out` parameter whose covariance is what lets a string schema stand in
    // for a schema of anything.
    let consumer = "// @flow
import { ZodString } from './input.d.ts.flow';
import type { ZodString as StringSchema, ZodType } from './input.d.ts.flow';
declare var schema: StringSchema;
const parsed: string = schema.parse(1);
const wrong: number = schema.parse(1);
const general: ZodType<mixed> = schema;
const instance = new ZodString();
instance.frobnicate();
";
    assert_eq!(consumer_errors("same-name", consumer), [6, 9]);
}

#[test]
fn an_object_type_is_inexact_and_its_members_are_checked() {
    let consumer = "// @flow
import type { Shape, Callable } from './input.d.ts.flow';
const extra: Shape = { kind: 'circle', area: () => 1, color: 'red' };
const wrong: Shape = { kind: 'triangle', area: () => 1 };
declare var call: Callable;
const size: string = call.size;
";
    assert_eq!(consumer_errors("interfaces", consumer), [4, 6]);
}

#[test]
fn an_enum_is_a_flow_enum() {
    let consumer = "// @flow
import { Direction, Color } from './input.d.ts.flow';
const up: Direction = Direction.Up;
const number: Direction = 0;
const red: Color = Color.Red;
const string: Color = 'RED';
";
    assert_eq!(consumer_errors("enums", consumer), [4, 6]);
}

#[test]
fn a_class_method_implementing_a_function_property_is_typed() {
    let consumer = "// @flow
import { AliasScheduler, IndexedScheduler, KeyofOmittedScheduler, OmittedScheduler, PickedScheduler, Scheduler, TransformScheduler, type MethodProperty, type OmitKeyof } from './input.d.ts.flow';
const scheduler = new Scheduler();
const ok: Promise<string> = scheduler.execute('job');
scheduler.execute(1);
const wrong: Promise<number> = scheduler.execute('job');
const omitted = new OmittedScheduler();
omitted.execute(1);
const keyofOmitted = new KeyofOmittedScheduler();
keyofOmitted.execute(1);
const picked = new PickedScheduler();
picked.execute(1);
const alias = new AliasScheduler();
alias.execute(1);
const transform = new TransformScheduler();
const transformed: Promise<number> = transform.transform('job');
transform.transform(1);
const transformedWrong: Promise<string> = transform.transform('job');
const indexed = new IndexedScheduler();
const indexedOk: Promise<number> = indexed.read('job');
indexed.read(1);
const indexedWrong: Promise<string> = indexed.read('job');
type MissingKeyOmit = OmitKeyof<MethodProperty, 'missing'>;
declare const missingKeyOmit: MissingKeyOmit;
";
    assert_eq!(
        consumer_errors("classes", consumer),
        [5, 6, 8, 10, 12, 14, 17, 18, 21, 22]
    );
}

#[test]
fn a_class_override_keeps_an_inherited_optional_parameter() {
    let consumer = "// @flow
import { RequiredParser, type ParserBase } from './input.d.ts.flow';
const parser: ParserBase = new RequiredParser();
parser.validate('value');
parser.validate('value', { strict: true });
parser.validate('value', { strict: 'yes' });
";
    assert_eq!(consumer_errors("class-override-optional", consumer), [6]);
}

#[test]
fn an_overload_a_guard_and_a_generic_bound_are_checked() {
    let consumer = "// @flow
import { parse, isString, generic } from './input.d.ts.flow';
const parsed: number = parse('1');
parse(1);
declare var value: mixed;
if (isString(value)) {
  const length: number = value.length;
  const wrong: number = value;
}
generic(1, 2);
";
    assert_eq!(consumer_errors("functions", consumer), [4, 8, 10]);
}

#[test]
fn a_tuple_default_satisfies_a_rest_argument_constraint() {
    let consumer = "// @flow
import type { Apply } from './input.d.ts.flow';
declare const apply: Apply<[string, number]>;
apply('value', 1);
apply('value');
apply('value', 'wrong');
";
    assert_eq!(consumer_errors("generics", consumer), [5, 6]);
}

#[test]
fn a_re_exported_class_and_a_split_name_resolve_through_the_chain() {
    let consumer = "// @flow
import { Model, MergedAgain, value } from './input.d.ts.flow';
import type { MergedAgain as MergedType, Kind } from './input.d.ts.flow';
const id: number = new Model().id;
const merged: MergedType = MergedAgain.create();
const kind: Kind = value;
const wrong: Kind = 'c';
";
    assert_eq!(consumer_errors("imports-and-re-exports", consumer), [4, 7]);
}

#[test]
fn a_type_export_star_cycle_is_checked() {
    let consumer = "// @flow
import type { FormatDistanceOptions, Locale, LocalizedOptions } from './input.d.ts.flow';
const locale: Locale = { code: 'en', formatDistance: { addSuffix: true } };
const options: LocalizedOptions<'formatDistance'> = { locale };
const wrong: FormatDistanceOptions = { addSuffix: 'yes' };
";
    assert_eq!(consumer_errors("type-export-star-cycle", consumer), [5]);
}

#[test]
fn an_export_assignment_is_module_exports_with_its_namespace_types() {
    let consumer = "// @flow
import createServer from './input.d.ts.flow';
import type { Options } from './input.d.ts.flow';
const options: Options = { port: 80 };
createServer({ port: 'eighty' });
createServer(options).listen();
";
    assert_eq!(consumer_errors("export-equals", consumer), [5]);
}
