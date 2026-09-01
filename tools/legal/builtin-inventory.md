# Builtin Inventory

uniflowed is MIT licensed. Builtin code that is copied, ported, generated, or
embedded must keep provenance and license evidence before it becomes runtime
code.

## Current Policy

- Prefer original Rust implementations owned by this repository.
- Keep wrappers lightweight and type-safe, with no vendored source unless there
  is an explicit license review.
- Do not copy source from reference projects while sketching API-compatible
  contracts.
- Keep generated declaration output traceable to the Rust source generator.
- Add third-party license text under `tools/legal/notices/` before vendoring or
  static linking external source.

## Reference Surfaces

| Area | Reference | Current use | License action |
| --- | --- | --- | --- |
| Runtime interoperability | WinterTC Minimum Common API | API alignment only | Cite spec in docs; no code copied |
| Standard library shape | Deno `@std` | Surface research only | No code copied |
| Native all-in-one runtime | Bun APIs | Product and performance reference only | No code copied |
| Node compatibility | Node.js APIs | Compatibility reference only | No code copied |
| Markdown | ox-content wasm | Planned builtin dependency or port | Review license before embedding |
| Flow parser | official Flow Rust port | Planned parser integration | Review license before linking |
| Hermes | Hermes runtime | Planned runtime integration | Review license before linking |
| Why3 / Z3 | formal verification tools | Tooling only | Keep under `tools/formal` |

## Required Checklist Before Adding A Builtin

- [ ] Record package/project name and upstream URL.
- [ ] Record license identifier and license file path.
- [ ] Confirm compatibility with the repository MIT license.
- [ ] Decide whether the dependency is linked, called as a tool, generated from,
      or vendored.
- [ ] Add notices for vendored or statically linked source.
- [ ] Add a test that proves the builtin does not rely on undeclared globals.
