#![deny(missing_docs)]
//! What `@uniflowed/tui` is, as the toolchain describes it.
//!
//! This crate holds no renderer. `@uniflowed/tui` is Flow-typed JavaScript on
//! React's own reconciler — the argument for that, and against a Rust core
//! with a binding, is in `packages/tui/index.js` where the code it decided
//! about lives. What is here is the description `uf inspect` prints and
//! `uf_lib`'s registry publishes: which standard the package follows, how it
//! renders, and — the part this crate exists to keep honest — exactly which
//! of OpenTUI's features and components it implements today.
//!
//! It said something else until ubugeeei-prod/uf#247. `engine` was
//! `"uf-native-open-tui-compatible"`, `renderer` was `"cell-diff-native"`, the
//! feature list named all twenty-three of OpenTUI's capabilities and the
//! component list all twenty-seven of its components, and the package behind
//! all of it was a hundred and ninety-four lines of Flow types over a function
//! that threw. A contract that describes an implementation nobody wrote is
//! worse than no contract, because a reader believes it.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Inline feature list for the TUI framework contract.
pub type TuiFeatureList = SmallVec<[TuiFeature; 24]>;

/// Inline component list for the TUI framework contract.
pub type TuiComponentList = SmallVec<[TuiComponent; 32]>;

/// Inline compound part list for a TUI component.
pub type TuiPartList = SmallVec<[CompactString; 8]>;

/// Native terminal UI framework contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiFrameworkContract {
    /// Native engine backing the terminal renderer.
    pub engine: TuiEngine,
    /// Compatibility standard followed by the component and renderer surface.
    pub standard: TuiStandard,
    /// Renderer update strategy.
    pub renderer: TuiRenderer,
    /// Layout engine used by boxes and components.
    pub layout: TuiLayoutEngine,
    /// Input model used for keyboard, mouse, focus, and selection.
    pub input: TuiInputModel,
    /// Runtime binding exposed to Flow and the uf runtime.
    pub runtime_binding: TuiRuntimeBinding,
    /// Feature matrix supported by the framework.
    pub features: TuiFeatureList,
    /// Component matrix exposed by the Flow package.
    pub components: TuiComponentList,
    /// Target that keeps the React Ink replacement bar explicit.
    pub react_ink_target: ReactInkTarget,
}

impl Default for TuiFrameworkContract {
    fn default() -> Self {
        Self {
            engine: TuiEngine::FlowReactOpenTuiCompatible,
            standard: TuiStandard::OpenTui,
            renderer: TuiRenderer::CellDiff,
            layout: TuiLayoutEngine::FlexboxCells,
            input: TuiInputModel::KeyboardMouseSelectionFocus,
            runtime_binding: TuiRuntimeBinding::FlowReact,
            // Ten, not twenty-three. Every one of these is exercised by
            // `packages/tui/tui.test.js` against a rendered frame; the other
            // thirteen variants of `TuiFeature` name parts of OpenTUI that uf
            // does not implement yet, and listing them here is how a reader
            // ends up importing a component that does not exist.
            features: smallvec::smallvec![
                TuiFeature::Flexbox,
                TuiFeature::CellDiff,
                TuiFeature::Keyboard,
                TuiFeature::Mouse,
                TuiFeature::Selection,
                TuiFeature::Focus,
                TuiFeature::RichText,
                TuiFeature::Scrollback,
                TuiFeature::InMemoryTesting,
                TuiFeature::SnapshotTesting,
            ],
            components: default_components(),
            react_ink_target: ReactInkTarget::default(),
        }
    }
}

impl TuiFrameworkContract {
    /// Return whether a feature is present in the framework contract.
    pub fn supports(&self, feature: TuiFeature) -> bool {
        self.features.contains(&feature)
    }

    /// Return whether a component with the given name is present.
    pub fn has_component(&self, name: &str) -> bool {
        self.components
            .iter()
            .any(|component| component.name == name)
    }

    /// Return a borrowed component descriptor by name.
    pub fn component(&self, name: &str) -> Option<&TuiComponent> {
        self.components
            .iter()
            .find(|component| component.name == name)
    }
}

/// What renders a terminal UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiEngine {
    /// Flow-typed JavaScript on React's reconciler, following OpenTUI.
    FlowReactOpenTuiCompatible,
}

/// Terminal UI compatibility standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiStandard {
    /// OpenTUI-compatible core, component, and testing semantics.
    OpenTui,
}

/// Renderer update strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiRenderer {
    /// A cell buffer, diffed against the last frame; only changed cells are sent.
    CellDiff,
}

/// TUI layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiLayoutEngine {
    /// Flexbox in whole cells, with OpenTUI's defaults and not Yoga's whole surface.
    ///
    /// It was `"flexbox-yoga-compatible"`, which claimed more than is true:
    /// `flexWrap`, `position: absolute` and `auto` margins are not implemented
    /// (ubugeeei-prod/uf#314), and a terminal resolves whole columns where Yoga
    /// resolves fractional pixels. What *is* compatible is the part a caller
    /// writes: the property names, and the defaults — a `flexDirection` that
    /// starts at `column`, an `alignItems` that starts at `stretch`, and a
    /// `flexShrink` that starts at zero for a numeric dimension.
    FlexboxCells,
}

/// TUI input model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiInputModel {
    /// Keyboard events to one declaratively focused node; mouse events to
    /// whatever is under the pointer, bubbling to its parents; and a drag over
    /// selectable text, which selects it.
    ///
    /// It said `keyboard-focus` until the mouse landed and `keyboard-mouse-focus`
    /// until selection did, and the parts are named separately because they are
    /// routed differently and a reader has to know which. Focus is a prop the
    /// application sets. A hit target is a fact about the frame that the
    /// renderer works out. A selection is two *cells* of that frame rather than
    /// a node at all, which is why it is a third name and not a detail of the
    /// second.
    ///
    /// Key *release* is still not implemented — it needs the Kitty keyboard
    /// protocol, and `KeyEvent.eventType` is always `"press"`. That is
    /// ubugeeei-prod/uf#314.
    KeyboardMouseSelectionFocus,
}

/// How the framework reaches an application's code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiRuntimeBinding {
    /// Flow declarations expose a React-compatible component surface.
    FlowReact,
}

/// One of OpenTUI's capabilities.
///
/// The variants are OpenTUI's whole vocabulary, because a contract has to be
/// able to *say* "mouse" in order to say uf does not implement it. What uf
/// implements is the list in `TuiFrameworkContract::default`, and those two
/// being different lengths is the point rather than an oversight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiFeature {
    /// Flexbox layout.
    Flexbox,
    /// Cell-diff renderer updates.
    CellDiff,
    /// Keyboard input.
    Keyboard,
    /// Mouse input.
    ///
    /// Press, release, motion, hover, drag with capture, drop and the wheel,
    /// routed to the node under the pointer and bubbling to its parents. Two
    /// things a reader might expect from the word are not behind it: there is
    /// no `zIndex`, so "topmost" means "painted last"; and reporting is off
    /// unless the application asks for it, because a terminal in mouse mode
    /// stops offering its own click-and-drag selection.
    Mouse,
    /// Focus management.
    Focus,
    /// Text selection.
    ///
    /// A left press on selectable text and a drag from it: the cells between
    /// the two are highlighted, `getSelectedText()` reads them back, and
    /// `event.preventDefault()` on the press is how a box that means its own
    /// thing by a drag keeps out of it. One selection per renderer, which is
    /// OpenTUI's rule and a terminal's.
    ///
    /// Three things a reader might expect from the word are not behind it. A
    /// selection is two *cells*, not a range inside the text, so what it
    /// covers after a commit is whatever the frame now holds there — the same
    /// answer a terminal's own selection gives, and stated because the DOM's
    /// is different. OpenTUI's repeated-press gestures, which widen a
    /// selection to the word or the logical line under the pointer, are not
    /// implemented; there is no `behavior` to report because there is only one.
    /// And *item* selection — `Select`, `TabSelect` — is a component rather
    /// than this, and is still ubugeeei-prod/uf#314.
    Selection,
    /// A window onto content taller than it, and a bar saying where.
    ///
    /// `ScrollBox`. What is *not* behind this is a terminal's own scrollback
    /// buffer. The wheel arrives — it is an `onMouseScroll` event like any
    /// other mouse event — but the offset is still a prop, so scrolling
    /// remains something the application does rather than something the
    /// component does to itself.
    ///
    /// What this word does promise, and what ubugeeei-prod/uf#314 asked to be
    /// proved rather than asserted, is that the *window* is what a scrolling
    /// box costs. Moving one over a hundred thousand rows measures nothing,
    /// lays out the rows on the screen, and walks into no others; a renderer
    /// that laid all hundred thousand out and clipped them would draw the
    /// same frames and is not this. It is counted rather than claimed —
    /// `tests/library/tui.test.js` counts the calls layout makes into its
    /// leaves — and the one pass still proportional to the content is the
    /// first frame, which has to ask every row its height once because the
    /// height of the content is what the offset gets clamped against.
    Scrollback,
    /// Key binding and command routing.
    Keymap,
    /// In-memory renderer for tests.
    InMemoryTesting,
    /// Snapshot testing for rendered terminal cells.
    SnapshotTesting,
    /// Terminal automation hooks.
    TerminalAutomation,
    /// Styled terminal text.
    RichText,
    /// Code highlighting.
    CodeHighlight,
    /// Markdown rendering.
    Markdown,
    /// Inline and protocol-aware images.
    Images,
    /// Sound and streaming audio hooks.
    Audio,
    /// 3D/WebGPU bridge for terminal canvases.
    ThreeD,
    /// SSH-hosted terminal applications.
    Ssh,
    /// QR code rendering.
    QrCode,
    /// Embedded terminal panes.
    EmbeddedTerminal,
    /// Clipboard API.
    Clipboard,
    /// Host notifications.
    Notifications,
    /// Timeline and animation API.
    Animations,
}

/// High-level component category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiComponentKind {
    /// Display and layout primitive.
    Display,
    /// Input primitive.
    Input,
    /// Selection primitive.
    Selection,
    /// Scrolling primitive.
    Scrolling,
    /// Rich content primitive.
    RichContent,
    /// Graphics and media primitive.
    Graphics,
    /// Application API primitive.
    Application,
    /// Testing primitive.
    Testing,
    /// Integration primitive.
    Integration,
}

/// Component descriptor for `@uniflowed/tui`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuiComponent {
    /// Component export name.
    pub name: CompactString,
    /// Compound parts such as `Root`, `Body`, or `Item`.
    pub parts: TuiPartList,
    /// Component category.
    pub kind: TuiComponentKind,
    /// Whether the declaration is safe to use from server-rendered descriptions.
    pub server_component_safe: bool,
    /// Whether the component needs client-side terminal interaction.
    pub interactive: bool,
    /// Primary feature backing the component.
    pub feature: TuiFeature,
}

impl TuiComponent {
    /// Create a component descriptor.
    pub fn new(
        name: &str,
        parts: &[&str],
        kind: TuiComponentKind,
        feature: TuiFeature,
        interactive: bool,
    ) -> Self {
        Self {
            name: name.to_compact_string(),
            parts: parts
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
            kind,
            server_component_safe: !interactive,
            interactive,
            feature,
        }
    }

    /// Return whether the component exposes a compound part.
    pub fn has_part(&self, part: &str) -> bool {
        self.parts.iter().any(|candidate| candidate == part)
    }
}

/// Where `@uniflowed/tui` stands next to React Ink.
///
/// Ink is the library a reader is coming from, so the useful thing to publish
/// is the gap rather than the ambition. Three of these five were `true` before
/// anything was implemented; they are what they are now, and each one becomes
/// true by somebody making it true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactInkTarget {
    /// Whether an Ink application could be ported without losing a capability.
    ///
    /// Not yet: Ink has no mouse either, but it does have `<Static>`, a
    /// spinner ecosystem and a component library uf has four components
    /// against. See ubugeeei-prod/uf#314.
    pub replacement_ready: bool,
    /// Whether rendering happens in native code rather than in JavaScript.
    ///
    /// It does not, and deliberately — `packages/tui/index.js` argues it out.
    /// The field stays because "is this native" is a question a reader of a
    /// toolchain asks, and a missing answer reads as yes.
    pub native_renderer: bool,
    /// Whether component props are exact Flow types rather than a loose bag.
    pub typed_components: bool,
    /// Whether images, audio, 3D, SSH and embedded terminals are in scope.
    ///
    /// In scope, not implemented. Nothing in the package draws a picture.
    pub rich_media: bool,
    /// Whether tests can render without a host terminal.
    pub in_memory_tests: bool,
    /// What the renderer's performance claim is.
    pub performance_target: TuiPerformanceTarget,
}

impl Default for ReactInkTarget {
    fn default() -> Self {
        Self {
            replacement_ready: false,
            native_renderer: false,
            typed_components: true,
            rich_media: false,
            in_memory_tests: true,
            performance_target: TuiPerformanceTarget::WritesOnlyChangedCells,
        }
    }
}

/// What the renderer's performance claim actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiPerformanceTarget {
    /// Putting a frame on the terminal costs the cells that changed, and no more.
    ///
    /// This replaces `"faster-than-react-ink"`, which was never measured
    /// against React Ink and so was not a claim, it was an ambition someone
    /// wrote down. This one is measured, by `packages/tui/tui.test.js`: in an
    /// 80×24 terminal, the first frame sends 1,920 cells and changing one
    /// character of a status line then sends **one cell in seven bytes** — a
    /// cursor move and the character.
    ///
    /// It is also now measured *against Ink*, which is ubugeeei-prod/uf#315
    /// and is `tools/bench/tui/`: one workload written twice, and the numbers
    /// on `docs/app/guide/tui/_uf.page.mdx` with the hardware, versions and
    /// variance `ubugeeei-redundancy.md` requires. The short version, and it
    /// is deliberately not all in uf's favour — on the same 80×24 frame,
    /// changing one character costs uf 8 bytes, Ink 1,307 by default and 128
    /// with `incrementalRendering` turned on; the **first** frame costs uf
    /// 2,102 bytes and Ink 1,122, because cell addressing is not free and this
    /// renderer pays for it up front. The wall clock does not separate the two
    /// at this size: both are waiting for React's scheduler.
    ///
    /// The name is still the claim, and it is still the narrow one. It says
    /// what putting a frame on a terminal costs. It does not say "faster than
    /// Ink", because on one of the four steps it is not.
    WritesOnlyChangedCells,
}

/// Return the default OpenTUI-compatible TUI framework contract.
pub fn contract() -> TuiFrameworkContract {
    TuiFrameworkContract::default()
}

/// The components `@uniflowed/tui` exports today.
///
/// Four, and the list is short on purpose. `Box` is a flex container that can
/// draw a background, a border and two titles; `Text` is styled text that
/// knows how to wrap; `Input` is a line somebody types into; `ScrollBox` is a
/// window onto content taller than itself. Everything else OpenTUI offers is
/// ubugeeei-prod/uf#314 rather than an entry here — a component named in a
/// contract and absent from the package is the failure this whole change is
/// about.
///
/// `ScrollBox` is marked as describing a frame rather than needing a keyboard,
/// and that is not a slip: its offset is a prop, so a server-rendered
/// description of one at a given row is a picture somebody can produce
/// anywhere. Binding a key to move that offset is the application's, and needs
/// `useKeyboard` like anything else.
fn default_components() -> TuiComponentList {
    use TuiComponentKind::{Display, Input, Scrolling};

    smallvec::smallvec![
        TuiComponent::new("Box", &["Root"], Display, TuiFeature::Flexbox, false),
        TuiComponent::new("Text", &["Root"], Display, TuiFeature::RichText, false),
        TuiComponent::new("Input", &["Root"], Input, TuiFeature::Keyboard, true),
        TuiComponent::new(
            "ScrollBox",
            &["Root"],
            Scrolling,
            TuiFeature::Scrollback,
            false,
        ),
    ]
}

#[cfg(test)]
mod tests;
