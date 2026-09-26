use std::hint::black_box;
use uf_infra::{LineIndex, append, cstr, normalize_slashes};
use uf_profiler::{CountingAllocator, ThreadWindow};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

#[test]
fn short_paths_and_compact_messages_do_not_allocate() {
    let window = ThreadWindow::open();
    for path in ["src/app.js", "src\\app.js", "日本語\\x.js", "\\a\\\\b\\"] {
        black_box(normalize_slashes(black_box(path)));
    }
    black_box(cstr!("item-{}", black_box(12)));
    assert_eq!(window.close().allocations, 0);
}

#[test]
fn appending_reuses_the_callers_allocation() {
    let mut output = String::with_capacity(4096);
    let window = ThreadWindow::open();
    for index in 0..64 {
        append!(output, "export const field{index} = {index};\n");
    }
    assert_eq!(window.close().allocations, 0);
    assert!(output.contains("field63 = 63"));
}

#[test]
fn short_line_indexes_do_not_allocate() {
    let window = ThreadWindow::open();
    let index = LineIndex::new(black_box("one\ntwo\n日本語\nfour\n"));
    assert_eq!(index.line_count(), 5);
    assert_eq!(index.line_col(8).line, 3);
    assert_eq!(window.close().allocations, 0);
}

#[test]
fn long_inputs_spill_without_changing_offsets_or_unicode() {
    let source = "日本語\n".repeat(128);
    let index = LineIndex::new(&source);
    assert_eq!(index.line_count(), 129);
    for line in 0..=128 {
        assert_eq!(index.line_col(line * 10).line, line + 1);
    }
    let path = "日本語\\nested\\".repeat(16);
    assert_eq!(normalize_slashes(&path).as_str(), path.replace('\\', "/"));
}
