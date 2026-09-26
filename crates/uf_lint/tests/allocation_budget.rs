#![allow(clippy::disallowed_macros)]

//! Allocation guards for `uf lint` over `npm/router/internal/runtime.js`.
//!
//! ubugeeei-prod/uf#668 was filed from this module, which used to make
//! `uf lint` build enough Babel-shaped tree data to allocate hundreds of
//! thousands of times for rules that could not report anything from it. The
//! exact number may move as the router grows, but with the React Compiler's
//! rules off this file must stay far below the old `serde_json::Value`
//! pipeline cost.
//!
//! With them on — the default — building that tree is the point. The
//! `react-compiler/*` rules report the official React Compiler's diagnostics,
//! and the compiler reads the Babel-shaped tree `uf build` builds for it, for
//! every module `eslint-plugin-react-hooks` would compile. This module declares
//! sixteen components and hooks, so the compiler runs, and that has a budget of
//! its own (ubugeeei-prod/uf#1140) rather than a loosened #668 one.

use uf_config::{RuleLevel, UniflowedConfig};
use uf_lint::{RuleCategory, SourceFile};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Above the current cost on `main` with room for allocator and fixture drift,
/// below the 293,000-allocation post-#708 figure from #668.
const RUNTIME_JS_ALLOCATIONS_PER_KIB_CEILING: u64 = 256;

/// Above the current 3.50 MiB run with room for fixture drift, below the
/// 32 MiB post-#708 figure from #668.
const RUNTIME_JS_BYTES_PER_BYTE_CEILING: u64 = 50;

/// With the `react-compiler/*` rules on: 709,167 allocations for the 68 KiB
/// fixture when this budget was set, about 10,430 per KiB, with 15% room for
/// fixture and compiler drift. Nearly all of it is the compiler's input — the
/// ESTree, Babel and typed trees `uf build` also builds — and the compile.
const COMPILER_ALLOCATIONS_PER_KIB_CEILING: u64 = 12_000;

/// With the `react-compiler/*` rules on: 137.7 MB allocated over the run
/// (not held at once) when this budget was set, about 1,982 bytes per source
/// byte, with 15% room.
const COMPILER_BYTES_PER_BYTE_CEILING: u64 = 2_300;

#[test]
fn runtime_js_lint_stays_below_the_babel_tree_allocation_budget() {
    let mut config = UniflowedConfig::default();
    for descriptor in uf_lint::rules() {
        if descriptor.category == RuleCategory::ReactCompiler {
            config
                .lint
                .rules
                .insert(descriptor.id.into(), RuleLevel::Off);
        }
    }
    let measured = measure(&config);
    measured.assert_within(
        RUNTIME_JS_ALLOCATIONS_PER_KIB_CEILING,
        RUNTIME_JS_BYTES_PER_BYTE_CEILING,
        "This usually means the #668 gate stopped skipping Babel/ESTree work that \
         cannot produce a lint finding.",
    );
}

#[test]
fn the_react_compiler_rules_stay_within_their_budget() {
    let measured = measure(&UniflowedConfig::default());
    measured.assert_within(
        COMPILER_ALLOCATIONS_PER_KIB_CEILING,
        COMPILER_BYTES_PER_BYTE_CEILING,
        "The React Compiler rules build the compiler's tree once per module and share \
         it with `react/no-redundant-memo`; a second build, or a second compile of the \
         same text, is the first thing to look for.",
    );
}

/// What one lint of the fixture cost under `config`.
struct Measured {
    source_bytes: u64,
    allocations: u64,
    bytes_allocated: u64,
}

impl Measured {
    fn assert_within(&self, allocations_per_kib: u64, bytes_per_byte: u64, hint: &str) {
        let allocation_ceiling = self.source_bytes.div_ceil(1024) * allocations_per_kib;
        let byte_ceiling = self.source_bytes * bytes_per_byte;
        let figures = format!(
            "linting {} bytes of router runtime took {} allocations and {} bytes, against \
             ceilings of {allocation_ceiling} allocations ({allocations_per_kib} per KiB) and \
             {byte_ceiling} bytes ({bytes_per_byte} per source byte). {hint} Run `cargo run \
             --release --example alloc_report -p uf_lint -- npm/router/internal/runtime.js \
             --phases` to find the phase.",
            self.source_bytes, self.allocations, self.bytes_allocated,
        );
        assert!(self.allocations <= allocation_ceiling, "{figures}");
        assert!(self.bytes_allocated <= byte_ceiling, "{figures}");
    }
}

fn measure(config: &UniflowedConfig) -> Measured {
    let source = std::fs::read_to_string(runtime_fixture()).expect("read router runtime fixture");
    let file = SourceFile {
        path: "npm/router/internal/runtime.js".to_owned(),
        source,
    };

    // Warm process-global parser/compiler state before measuring the lint path
    // itself. The issue's `alloc_report` example does the same. Under another
    // path, because `uf_transform::lint` remembers a module's compiler findings
    // by path and text: a measured run answered from that would measure none of
    // what the React Compiler rules cost.
    let warm_file = SourceFile {
        path: "npm/router/internal/runtime-warm.js".to_owned(),
        source: file.source.clone(),
    };
    let warm = uf_lint::lint_source(&warm_file, config).expect("warm lint");
    assert!(
        warm.diagnostics.is_empty(),
        "the fixture under measurement should lint cleanly, got {:#?}",
        warm.diagnostics
    );

    // This thread, and the thread `uf_lint` parses the module on, which hands
    // its allocations back — not any other thread in the binary. See
    // `uf_profiler::Handover`.
    let window = ThreadWindow::open();
    let report = uf_lint::lint_source(&file, config).expect("measured lint");
    let delta = window.close();

    assert!(
        report.diagnostics.is_empty(),
        "the fixture under measurement should lint cleanly, got {:#?}",
        report.diagnostics
    );

    // Linting this module parses it, on the module-tree thread, and that thread
    // hands its allocations back to the window above. A figure smaller than
    // one parse of the fixture costs by itself is therefore a measurement that
    // did not see the thread — and every ceiling would pass whatever the thread
    // did.
    let parse = parse_allocations(&file.source);
    assert!(
        delta.allocations >= parse,
        "linting router runtime took {} allocations, fewer than the {parse} that parsing \
         it takes on its own. Either the module-tree thread's allocations no longer reach \
         this measurement — see `uf_profiler::Handover` in `run_module_tree_rules` — or \
         `uf lint` stopped parsing this fixture, and the budget needs one it does parse.",
        delta.allocations,
    );

    Measured {
        source_bytes: u64::try_from(file.source.len()).expect("source length fits in u64"),
        allocations: delta.allocations,
        bytes_allocated: delta.bytes_allocated,
    }
}

fn runtime_fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../npm/router/internal/runtime.js")
}

/// What `uf_flow::parse` of `source` allocates by itself, counted on a thread
/// with the stack the parser needs.
fn parse_allocations(source: &str) -> u64 {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || {
                let window = ThreadWindow::open();
                let parsed = uf_flow::parse(source).expect("the fixture parses");
                let delta = window.close();
                drop(parsed);
                delta.allocations
            })
            .expect("a parse thread starts")
            .join()
            .expect("the parse thread survives")
    })
}
