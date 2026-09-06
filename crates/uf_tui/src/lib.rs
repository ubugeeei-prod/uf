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
            input: TuiInputModel::KeyboardFocus,
            runtime_binding: TuiRuntimeBinding::FlowReact,
            // Seven, not twenty-three. Every one of these is exercised by
            // `tests/library/tui.test.js` against a rendered frame; the other
            // sixteen variants of `TuiFeature` name parts of OpenTUI that uf
            // does not implement yet, and listing them here is how a reader
            // ends up importing a component that does not exist.
            features: smallvec::smallvec![
                TuiFeature::Flexbox,
                TuiFeature::CellDiff,
                TuiFeature::Keyboard,
                TuiFeature::Focus,
                TuiFeature::RichText,
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
    /// Keyboard events, routed to one declaratively focused node.
    ///
    /// Mouse reporting and text selection are OpenTUI's other two input
    /// sources and are not implemented; they are ubugeeei-prod/uf#314. This
    /// value used to say they were.
    KeyboardFocus,
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
    Mouse,
    /// Focus management.
    Focus,
    /// Text and item selection.
    Selection,
    /// Scrollback buffers.
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
    /// spinner ecosystem and a component library uf has three components
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
    /// wrote down. This one is measured, by `tests/library/tui.test.js`: in an
    /// 80×24 terminal, the first frame sends 1,920 cells and changing one
    /// character of a status line then sends **one cell in seven bytes** — a
    /// cursor move and the character. A renderer that diffs *lines* has to
    /// resend everything below the change.
    ///
    /// The comparison against Ink itself is still worth having, and is
    /// ubugeeei-prod/uf#315: it needs Ink installed, a workload both libraries
    /// can render, and the hardware, versions and variance that
    /// `ubugeeei-redundancy.md` requires of a performance claim.
    WritesOnlyChangedCells,
}

/// Return the default OpenTUI-compatible TUI framework contract.
pub fn contract() -> TuiFrameworkContract {
    TuiFrameworkContract::default()
}

/// The components `@uniflowed/tui` exports today.
///
/// Three, and the list is short on purpose. `Box` is a flex container that can
/// draw a background, a border and two titles; `Text` is styled text that
/// knows how to wrap; `Input` is a line somebody types into. Everything else
/// OpenTUI offers is built from those plus state, and it is
/// ubugeeei-prod/uf#314 rather than an entry here — a component named in a
/// contract and absent from the package is the failure this whole change is
/// about.
fn default_components() -> TuiComponentList {
    use TuiComponentKind::{Display, Input};

    smallvec::smallvec![
        TuiComponent::new("Box", &["Root"], Display, TuiFeature::Flexbox, false),
        TuiComponent::new("Text", &["Root"], Display, TuiFeature::RichText, false),
        TuiComponent::new("Input", &["Root"], Input, TuiFeature::Keyboard, true),
    ]
}

#[cfg(test)]
mod tests;
