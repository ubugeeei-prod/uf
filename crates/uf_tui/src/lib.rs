#![deny(missing_docs)]
//! Native OpenTUI-compatible terminal UI contracts for `@uniflowed/tui`.

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
            engine: TuiEngine::UfNativeOpenTuiCompatible,
            standard: TuiStandard::OpenTui,
            renderer: TuiRenderer::CellDiffNative,
            layout: TuiLayoutEngine::FlexboxYogaCompatible,
            input: TuiInputModel::KeyboardMouseFocusSelection,
            runtime_binding: TuiRuntimeBinding::FlowReact,
            features: smallvec::smallvec![
                TuiFeature::Flexbox,
                TuiFeature::CellDiff,
                TuiFeature::Keyboard,
                TuiFeature::Mouse,
                TuiFeature::Focus,
                TuiFeature::Selection,
                TuiFeature::Scrollback,
                TuiFeature::Keymap,
                TuiFeature::InMemoryTesting,
                TuiFeature::SnapshotTesting,
                TuiFeature::TerminalAutomation,
                TuiFeature::RichText,
                TuiFeature::CodeHighlight,
                TuiFeature::Markdown,
                TuiFeature::Images,
                TuiFeature::Audio,
                TuiFeature::ThreeD,
                TuiFeature::Ssh,
                TuiFeature::QrCode,
                TuiFeature::EmbeddedTerminal,
                TuiFeature::Clipboard,
                TuiFeature::Notifications,
                TuiFeature::Animations,
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

/// Native TUI engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiEngine {
    /// uf native renderer following OpenTUI-compatible semantics.
    UfNativeOpenTuiCompatible,
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
    /// Native framebuffer renderer that writes only changed cells.
    CellDiffNative,
}

/// TUI layout engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiLayoutEngine {
    /// Yoga-compatible flexbox layout.
    FlexboxYogaCompatible,
}

/// TUI input model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiInputModel {
    /// Keyboard, mouse, focus, and selection events use one typed model.
    KeyboardMouseFocusSelection,
}

/// TUI runtime binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiRuntimeBinding {
    /// Flow declarations expose a React-compatible component surface.
    FlowReact,
}

/// TUI capability exposed by the default framework.
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

/// React Ink replacement target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactInkTarget {
    /// Whether the API is intended to replace React Ink for uf applications.
    pub replacement_ready: bool,
    /// Whether rendering happens in native code instead of JavaScript text diffing.
    pub native_renderer: bool,
    /// Whether component props are meant to be generated as exact Flow types.
    pub typed_components: bool,
    /// Whether graphics, media, SSH, QR, and embedded terminal use cases are in scope.
    pub rich_media: bool,
    /// Whether tests can render without a host terminal.
    pub in_memory_tests: bool,
    /// Performance target for the renderer.
    pub performance_target: TuiPerformanceTarget,
}

impl Default for ReactInkTarget {
    fn default() -> Self {
        Self {
            replacement_ready: true,
            native_renderer: true,
            typed_components: true,
            rich_media: true,
            in_memory_tests: true,
            performance_target: TuiPerformanceTarget::FasterThanReactInk,
        }
    }
}

/// TUI performance target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuiPerformanceTarget {
    /// Target better throughput and latency than React Ink for large terminal UIs.
    FasterThanReactInk,
}

/// Return the default OpenTUI-compatible TUI framework contract.
pub fn contract() -> TuiFrameworkContract {
    TuiFrameworkContract::default()
}

fn default_components() -> TuiComponentList {
    use TuiComponentKind::{
        Application, Display, Graphics, Input, Integration, RichContent, Scrolling, Selection,
        Testing,
    };

    smallvec::smallvec![
        TuiComponent::new("Box", &["Root"], Display, TuiFeature::Flexbox, false),
        TuiComponent::new("Text", &["Root"], Display, TuiFeature::RichText, false),
        TuiComponent::new("Input", &["Root"], Input, TuiFeature::Keyboard, true),
        TuiComponent::new("Textarea", &["Root"], Input, TuiFeature::Keyboard, true),
        TuiComponent::new(
            "Select",
            &["Root", "Item", "Group", "Empty"],
            Selection,
            TuiFeature::Selection,
            true,
        ),
        TuiComponent::new(
            "TabSelect",
            &["Root", "Tab", "Panel"],
            Selection,
            TuiFeature::Selection,
            true,
        ),
        TuiComponent::new(
            "Slider",
            &["Root", "Track", "Thumb"],
            Input,
            TuiFeature::Mouse,
            true
        ),
        TuiComponent::new(
            "ScrollBox",
            &["Root", "Viewport", "Content"],
            Scrolling,
            TuiFeature::Scrollback,
            true,
        ),
        TuiComponent::new(
            "ScrollBar",
            &["Root", "Thumb"],
            Scrolling,
            TuiFeature::Scrollback,
            true
        ),
        TuiComponent::new(
            "Code",
            &["Root"],
            RichContent,
            TuiFeature::CodeHighlight,
            false
        ),
        TuiComponent::new(
            "Markdown",
            &["Root"],
            RichContent,
            TuiFeature::Markdown,
            false
        ),
        TuiComponent::new(
            "LineNumbers",
            &["Root"],
            RichContent,
            TuiFeature::CodeHighlight,
            false
        ),
        TuiComponent::new(
            "Diff",
            &["Root", "Hunk", "Line"],
            RichContent,
            TuiFeature::CodeHighlight,
            false
        ),
        TuiComponent::new(
            "TextTable",
            &["Root", "Row", "Cell"],
            RichContent,
            TuiFeature::RichText,
            false
        ),
        TuiComponent::new(
            "AsciiFont",
            &["Root"],
            Graphics,
            TuiFeature::RichText,
            false
        ),
        TuiComponent::new(
            "FrameBuffer",
            &["Root", "Layer"],
            Graphics,
            TuiFeature::Images,
            false
        ),
        TuiComponent::new("Image", &["Root"], Graphics, TuiFeature::Images, false),
        TuiComponent::new("QrCode", &["Root"], Graphics, TuiFeature::QrCode, false),
        TuiComponent::new(
            "EmbeddedTerminal",
            &["Root", "Session"],
            Integration,
            TuiFeature::EmbeddedTerminal,
            true,
        ),
        TuiComponent::new(
            "Clipboard",
            &["Root"],
            Application,
            TuiFeature::Clipboard,
            true
        ),
        TuiComponent::new(
            "Notification",
            &["Root"],
            Application,
            TuiFeature::Notifications,
            true,
        ),
        TuiComponent::new(
            "Audio",
            &["Root", "Stream"],
            Application,
            TuiFeature::Audio,
            true
        ),
        TuiComponent::new(
            "Timeline",
            &["Root", "Track"],
            Application,
            TuiFeature::Animations,
            true
        ),
        TuiComponent::new(
            "Keymap",
            &["Root", "Binding"],
            Application,
            TuiFeature::Keymap,
            true
        ),
        TuiComponent::new(
            "SshHost",
            &["Root", "Session"],
            Integration,
            TuiFeature::Ssh,
            true
        ),
        TuiComponent::new("ThreeCanvas", &["Root"], Graphics, TuiFeature::ThreeD, true),
        TuiComponent::new(
            "TestRenderer",
            &["Root"],
            Testing,
            TuiFeature::InMemoryTesting,
            false
        ),
    ]
}

#[cfg(test)]
mod tests;
