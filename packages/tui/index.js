// @flow
//
// `@uniflowed/tui`: a React renderer whose host is a terminal.
//
// It follows OpenTUI, which is the terminal-UI library uf's declaration named
// as its standard: the same component vocabulary, the same flexbox defaults
// (`flexDirection` starts at `"column"`), the same canonical key names
// (`"return"`, not `"enter"`), the same rule that focus is a prop rather than
// a Tab traversal the library performs for you, and the same split between a
// root that owns a React tree and a renderer that owns a terminal. A component
// written against OpenTUI's documentation behaves the same way here, which is
// the only thing "compatible with a standard" can usefully mean.
//
// # The decision: this is JavaScript, and the declaration used to say Rust
//
// The declaration this package replaced said `engine:
// "uf-native-open-tui-compatible"` and `renderer: "cell-diff-native"`, which
// is a promise that the renderer would be Rust with a binding. It is not, and
// this is the argument, because "faster than Ink" and "no native bindings"
// genuinely pull in opposite directions and the reason to pick one belongs in
// the source rather than in a pull request nobody will read again.
//
// **Rust was rejected for three reasons, in order of weight.**
//
// *First, there is no bridge, and building one is a bigger and different
// project than this.* Every `nativeRuntimeRequired` in `packages/` is a
// promise of a JavaScript-to-native boundary that this repository does not
// have: no Node-API addon, no FFI, no prebuilt platform binaries, no loader.
// Choosing Rust here would have meant that the first deliverable was that
// bridge and the second was a renderer, and until both existed
// `@uniflowed/tui` would still have been a declaration. Replacing one
// declaration with a differently-worded declaration is exactly the outcome
// ubugeeei-prod/uf#247 exists to prevent.
//
// *Second, the rule that keeps Effect and Validator in Flow applies here for
// its own reason rather than by its letter.* `ubugeeei-redundancy.md` names
// Effect, Validator, state, immutable updates, forms, hooks and UI, and does
// not name the TUI — but the reason it names them is that application-facing
// libraries get *deployed*, to browsers and edge workers where a Rust binary
// cannot go. A terminal application is the one case where that argument is
// weakest: it runs where a terminal is, and a machine with a terminal can run
// a binary. What survives is the smaller version of the same point. A native
// dependency means prebuilt binaries for every platform uf supports, a
// fallback for the ones it does not, and an `npm install` that can fail in a
// way a Flow package cannot — for a library whose whole job runs at human
// reading speed.
//
// *Third, and most concretely: the thing that makes a terminal UI slow is not
// JavaScript.* The guide's native-hot-path rule is about "repeated,
// repository-wide or CPU-intensive work", and says the execution phase decides
// the boundary. This executes in one process, at one terminal's size, at the
// rate a person presses keys. An 80×24 terminal is 1,920 cells; a large one is
// 12,000. Laying out and painting that is microseconds in any language. What
// costs milliseconds is the bytes handed to the terminal, because the emulator
// on the other end parses and re-renders them — which is why React Ink,
// written in JavaScript and using WebAssembly Yoga for the part that is
// supposedly slow, is slow for a reason neither of those explains: it renders
// to a *string* and reprints from the first changed line to the bottom of the
// frame. `diff.js` writes the cells that changed and nothing else, and that is
// an algorithm, not a language.
//
// The honest cost of this decision is written down rather than hidden: this
// package depends on `react-reconciler`, which React publishes for custom
// renderers and calls experimental, and which pins itself to a React minor.
// `internal/host.js` says what that means for a React upgrade.
//
// # What is here, and what is not
//
// Implemented, tested, and true: a component tree, flexbox layout in whole
// cells, a cell buffer with correct wide-grapheme handling, a diff that emits
// only changed cells, keyboard input with OpenTUI's key names and propagation
// rules, bracketed paste, declarative focus, mouse input — press, release,
// hover, drag with capture, drop and wheel, routed by a hit grid the painter
// records — a scrolling window onto content taller than it, terminal
// capability and *size* detection that agrees with the CLI's, and an in-memory
// renderer that runs the same code the terminal one does.
//
// Not here: text selection, key *release* (which needs the Kitty keyboard
// protocol), images, the rich content components, and everything under
// OpenTUI's "application APIs". They are ubugeeei-prod/uf#314, and they are
// absent rather than present as functions that throw — because a stub is what
// this package used to be.
//
// Also not here, and worth saying because ubugeeei-prod/uf#247 asked for it:
// uf's own CLI does not draw through this. It cannot — `crates/uf_term` is
// Rust, this is JavaScript, and there is no way to run a Flow program in this
// repository outside `uf test`, `uf dev` and `uf build`. ubugeeei-prod/uf#316
// is that gap, what would close it, and why the two renderers are each right
// for their own caller in the meantime. `tools/bench/tui/startup.js` now
// measures the number that issue says has to exist first: a Flow entry point
// that draws one frame costs 149 ms with a warm transform cache and 383 ms
// with a cold one, against 8 ms for the whole of `uf info` and 58 ms for
// `node -e 0`. A banner cannot be written this way; a session that already
// starts Node and then runs for minutes can.
//
// What the two renderers must not do is silently disagree about the terminal,
// and there are three guards rather than a promise. `capability.js` reproduces
// `crates/uf_term/src/capability.rs`'s colour precedence and its size
// precedence, `widths.js` holds the same Unicode tables as
// `crates/uf_term/src/text/tables.rs`, and `tests/library/tui.test.js` reads
// both Rust files and fails when either side is edited alone.
//
// # How the package is laid out
//
// Bottom to top, each module named for the one question it answers:
//
// - `widths.js` — how many columns a grapheme occupies.
// - `cells.js` — what a frame is: the grid, the colours, the continuation cell.
// - `layout.js` — flexbox, in whole cells.
// - `diff.js` — two frames, as the bytes that turn one into the other.
// - `keys.js` — terminal bytes, as key events, and a paste as one of them.
// - `mouse.js` — the other half of that stream: what the pointer did.
// - `capability.js` — what this terminal can render and how big it is, by the
//   CLI's own rules.
// - `terminal.js` — a real terminal, and the in-memory one tests use.
// - `components.js` — `Box`, `Text`, `Input`, `ScrollBox`, and the hooks.
//
// `internal/` holds the four that a consumer must not be able to reach past:
// `tree.js` (props become a layout style once, here), `paint.js` (both passes
// must break lines the same way), `hits.js` (what is under the pointer is only
// true for the frame that recorded it) and `host.js` (one React root, one
// terminal, one owner). Each says so in its own header. There is no
// `internal/util.js`: a module that cannot say what it is about does not
// belong in this package.

export type {
  BorderGlyphs,
  BorderStyle,
  Capabilities,
  ColorChoice,
  ColorLevel,
  GlyphSet,
  TerminalEnv,
  TerminalReport,
  TerminalSize,
  Tty,
} from "./capability.js";
export {
  FALLBACK_COLUMNS,
  FALLBACK_ROWS,
  borderGlyphs,
  detectCapabilities,
  detectSize,
  plainCapabilities,
} from "./capability.js";

export type { Color, Frame, Rect, Style } from "./cells.js";
export { Attributes, INHERIT, frameRow, frameText, parseColor } from "./cells.js";

export type {
  AlignItems,
  AlignSelf,
  Dimension,
  FlexDirection,
  JustifyContent,
  LayoutStyle,
  Overflow,
} from "./layout.js";

export type { WrapMode } from "./internal/paint.js";

export type { Update } from "./diff.js";

export type { InputDecoder, InputEvent, KeyEvent, KeySource } from "./keys.js";
export { createInputDecoder, decodeInput, decodeKeys } from "./keys.js";

export type { MouseEvent, MouseEventType, Scroll, ScrollDirection } from "./mouse.js";
export { MouseButton } from "./mouse.js";

export type { Renderer, Root } from "./internal/host.js";

export type { Handle, InputStream, OutputStream, RenderOptions, TestHandle } from "./terminal.js";
export { render, testRender } from "./terminal.js";

export type {
  BoxLayoutProps,
  BoxProps,
  ColorValue,
  InputProps,
  MouseProps,
  ScrollBoxProps,
  TextProps,
  TextStyleProps,
  TitleAlignment,
} from "./components.js";
export {
  Box,
  Input,
  ScrollBox,
  Text,
  useKeyboard,
  useRenderer,
  useTerminalSize,
} from "./components.js";
