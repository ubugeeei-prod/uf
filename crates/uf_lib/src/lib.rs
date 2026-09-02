use compact_str::{CompactString, ToCompactString};
use serde::Serialize;
use uf_infra::InlineVec;
use uf_motion::MotionContract;
use uf_orm::OrmContract;
use uf_std::{StdModule, std_modules};
use uf_tui::TuiFrameworkContract;
use uf_validator::{SchemaKind, ValidationStep};
use uf_vrt::VisualRegressionPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeModuleKind {
    Data,
    Effect,
    Framework,
    Hooks,
    Runtime,
    Std,
    Style,
    Testing,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stability {
    Experimental,
    Planned,
    Stable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeModule {
    pub specifier: CompactString,
    pub kind: NativeModuleKind,
    pub stability: Stability,
    pub flow_exports: InlineVec<CompactString, 8>,
}

impl NativeModule {
    pub fn new(
        specifier: &str,
        kind: NativeModuleKind,
        stability: Stability,
        exports: &[&str],
    ) -> Self {
        Self {
            specifier: specifier.to_compact_string(),
            kind,
            stability,
            flow_exports: exports
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
        }
    }
}

pub fn builtin_modules() -> Vec<NativeModule> {
    vec![
        NativeModule::new(
            "@uniflowed/core",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &[
                "describe",
                "it",
                "test",
                "expect",
                "beforeEach",
                "afterEach",
            ],
        ),
        NativeModule::new(
            "@uniflowed/react",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["React", "Suspense", "use", "cache"],
        ),
        NativeModule::new(
            "@uniflowed/react-native",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["View", "Text", "Platform"],
        ),
        NativeModule::new(
            "@uniflowed/testing",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["describe", "it", "test", "expect", "render", "screen"],
        ),
        NativeModule::new(
            "@uniflowed/test",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["describe", "it", "test", "expect", "bench", "plan"],
        ),
        NativeModule::new(
            "@uniflowed/react-testing",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["render", "screen", "fireEvent", "userEvent", "waitFor"],
        ),
        NativeModule::new(
            "@uniflowed/hooks",
            NativeModuleKind::Hooks,
            Stability::Experimental,
            &["useAsync", "useEvent", "useMediaQuery", "useStableCallback"],
        ),
        NativeModule::new(
            "@uniflowed/router",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &[
                "FileRoute",
                "loader",
                "action",
                "redirect",
                "useRoute",
                "useRouter",
                "defineNavigationGuard",
            ],
        ),
        NativeModule::new(
            "@uniflowed/server",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["serverAction", "headers", "cookies", "cache"],
        ),
        NativeModule::new(
            "@uniflowed/query",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["createQuery", "createMutation", "QueryClient", "useQuery"],
        ),
        NativeModule::new(
            "@uniflowed/fetch",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["ofetch", "createFetch", "request"],
        ),
        NativeModule::new(
            "@uniflowed/loader",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["createLoader", "useLoader", "loaderCell", "preload"],
        ),
        NativeModule::new(
            "@uniflowed/effect",
            NativeModuleKind::Effect,
            Stability::Experimental,
            &[
                "succeed",
                "fail",
                "die",
                "sync",
                "suspend",
                "promise",
                "tryPromise",
                "map",
                "mapError",
                "flatMap",
                "andThen",
                "zip",
                "all",
                "forEach",
                "race",
                "catchAll",
                "catchTag",
                "orElse",
                "either",
                "retry",
                "timeout",
                "acquireRelease",
                "scoped",
                "tag",
                "provide",
                "provideService",
                "fork",
                "join",
                "interrupt",
                "runPromise",
                "runSyncExit",
                "runFork",
            ],
        ),
        NativeModule::new(
            "@uniflowed/orm",
            NativeModuleKind::Data,
            Stability::Experimental,
            &[
                "defineTable",
                "column",
                "relation",
                "query",
                "migration",
                "db",
            ],
        ),
        NativeModule::new(
            "@uniflowed/relay",
            NativeModuleKind::Data,
            Stability::Experimental,
            &[
                "graphql",
                "useFragment",
                "useLazyLoadQuery",
                "commitMutation",
            ],
        ),
        NativeModule::new(
            "@uniflowed/graphql",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["graphql", "createGraphQlClient", "useLazyLoadQuery"],
        ),
        NativeModule::new(
            "@uniflowed/web",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &[
                "Font",
                "Image",
                "OgImage",
                "Link",
                "Page",
                "Layout",
                "Time",
                "Announcer",
                "Picture",
                "useCookie",
                "useHead",
            ],
        ),
        NativeModule::new(
            "@uniflowed/markdown",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["Markdown", "renderMarkdown", "compileMarkdown"],
        ),
        NativeModule::new(
            "@uniflowed/temporal",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["Temporal", "Instant", "PlainDate", "ZonedDateTime"],
        ),
        NativeModule::new(
            "@uniflowed/pwa",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["definePwa", "Manifest", "ServiceWorker"],
        ),
        NativeModule::new(
            "@uniflowed/prepare",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &["prepare", "lintStaged", "codegen"],
        ),
        NativeModule::new(
            "@uniflowed/pm",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &["inferFromConfig", "install", "upgrade", "lock", "store"],
        ),
        NativeModule::new(
            "@uniflowed/rm",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &["inferRuntime", "acquireRuntime", "applyRuntime", "doctor"],
        ),
        NativeModule::new(
            "@uniflowed/flow-cell",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["cell", "computed", "resource"],
        ),
        NativeModule::new(
            "@uniflowed/state",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["cell", "computed", "resource", "useCell"],
        ),
        NativeModule::new(
            "@uniflowed/validator",
            NativeModuleKind::Data,
            Stability::Experimental,
            &["v", "string", "number", "object", "pipe"],
        ),
        NativeModule::new(
            "@uniflowed/mock",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["mock", "get", "post", "use"],
        ),
        NativeModule::new(
            "@uniflowed/browser",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["browser", "viewport", "visit", "screenshot"],
        ),
        NativeModule::new(
            "@uniflowed/story",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["story", "variant", "withMocks", "withBrowser"],
        ),
        NativeModule::new(
            "@uniflowed/vrt",
            NativeModuleKind::Testing,
            Stability::Experimental,
            &["plan", "snapshot", "diff", "baseline"],
        ),
        NativeModule::new(
            "@uniflowed/motion",
            NativeModuleKind::Ui,
            Stability::Experimental,
            &["motion", "animate", "timeline", "spring", "reducedMotion"],
        ),
        NativeModule::new(
            "@uniflowed/tui",
            NativeModuleKind::Ui,
            Stability::Experimental,
            &[
                "Box",
                "Text",
                "Input",
                "Select",
                "ScrollBox",
                "FrameBuffer",
                "renderTui",
                "contract",
            ],
        ),
        NativeModule::new(
            "@uniflowed/cli",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &["defineCli", "command", "arg", "option"],
        ),
        NativeModule::new(
            "@uniflowed/std",
            NativeModuleKind::Std,
            Stability::Experimental,
            &[
                "modules",
                "wintertc",
                "vfs",
                "fs",
                "crypto",
                "http",
                "buffer",
                "importMeta",
                "defer",
            ],
        ),
        NativeModule::new(
            "@uniflowed/stylex",
            NativeModuleKind::Style,
            Stability::Experimental,
            &["stylex", "defineVars", "createTheme"],
        ),
        NativeModule::new(
            "@uniflowed/ui",
            NativeModuleKind::Ui,
            Stability::Experimental,
            &["Button", "Dialog", "Form", "Table", "Tabs", "Tooltip"],
        ),
        NativeModule::new(
            "@uniflowed/react-compiler",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["compiler", "syntaxMode"],
        ),
        NativeModule::new(
            "@uniflowed/runtime",
            NativeModuleKind::Runtime,
            Stability::Planned,
            &["run", "resolve", "spawn"],
        ),
        NativeModule::new(
            "@uniflowed/lib",
            NativeModuleKind::Runtime,
            Stability::Experimental,
            &["modules", "hooks", "ui", "version"],
        ),
        NativeModule::new(
            "@uniflowed/lint",
            NativeModuleKind::Framework,
            Stability::Experimental,
            &["defineRule", "typeAwareRule", "reactRule"],
        ),
    ]
}

pub fn std_module_descriptors() -> Vec<StdModule> {
    std_modules().into_iter().collect()
}

pub fn orm_contract() -> OrmContract {
    OrmContract::default()
}

pub fn motion_contract() -> MotionContract {
    MotionContract::default()
}

pub fn tui_contract() -> TuiFrameworkContract {
    TuiFrameworkContract::default()
}

pub fn vrt_plan() -> VisualRegressionPlan {
    VisualRegressionPlan::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDescriptor {
    pub name: CompactString,
    pub idempotent_render: bool,
    pub server_component_safe: bool,
}

impl HookDescriptor {
    pub fn new(name: &str, idempotent_render: bool, server_component_safe: bool) -> Self {
        Self {
            name: name.to_compact_string(),
            idempotent_render,
            server_component_safe,
        }
    }
}

pub fn hook_descriptors() -> Vec<HookDescriptor> {
    vec![
        HookDescriptor::new("useAsync", true, false),
        HookDescriptor::new("useDebouncedValue", true, false),
        HookDescriptor::new("useEvent", true, false),
        HookDescriptor::new("useInterval", true, false),
        HookDescriptor::new("useIsomorphicLayoutEffect", true, false),
        HookDescriptor::new("useLocalStorage", true, false),
        HookDescriptor::new("useMediaQuery", true, false),
        HookDescriptor::new("useMounted", true, false),
        HookDescriptor::new("usePrevious", true, false),
        HookDescriptor::new("useStableCallback", true, false),
        HookDescriptor::new("useServerValue", true, true),
    ]
}

pub fn module_by_specifier(specifier: &str) -> Option<NativeModule> {
    builtin_modules()
        .into_iter()
        .find(|module| module.specifier == specifier)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UiRuntime {
    Server,
    Client,
    Split,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiComponent {
    pub name: CompactString,
    pub parts: InlineVec<CompactString, 16>,
    pub runtime: UiRuntime,
    pub preset_style: bool,
    pub form: Option<FormContract>,
}

impl UiComponent {
    pub fn new(name: &str, parts: &[&str], runtime: UiRuntime) -> Self {
        Self {
            name: name.to_compact_string(),
            parts: parts
                .iter()
                .map(ToCompactString::to_compact_string)
                .collect(),
            runtime,
            preset_style: true,
            form: None,
        }
    }

    pub fn with_form(mut self, form: FormContract) -> Self {
        self.form = Some(form);
        self
    }

    pub fn has_part(&self, part: &str) -> bool {
        self.parts.iter().any(|candidate| candidate == part)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormContract {
    pub validator_module: CompactString,
    pub schema_kind: SchemaKind,
    pub allowed_steps: InlineVec<ValidationStep, 8>,
    pub compiler_safe: bool,
    pub render_idempotent: bool,
    pub mutation_phase: FormMutationPhase,
}

impl FormContract {
    pub fn validator_backed() -> Self {
        Self {
            validator_module: CompactString::const_new("@uniflowed/validator"),
            schema_kind: SchemaKind::Object,
            allowed_steps: InlineVec::new(),
            compiler_safe: true,
            render_idempotent: true,
            mutation_phase: FormMutationPhase::EventOrServerAction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormMutationPhase {
    EventOrServerAction,
}

pub fn ui_components() -> Vec<UiComponent> {
    vec![
        UiComponent::new(
            "Accordion",
            &["Root", "Item", "Trigger", "Content"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Alert",
            &["Root", "Title", "Description"],
            UiRuntime::Server,
        ),
        UiComponent::new(
            "AlertDialog",
            &[
                "Root",
                "Trigger",
                "Body",
                "Header",
                "Footer",
                "Title",
                "Description",
                "Action",
                "Cancel",
            ],
            UiRuntime::Split,
        ),
        UiComponent::new("AspectRatio", &["Root"], UiRuntime::Server),
        UiComponent::new("Avatar", &["Root", "Image", "Fallback"], UiRuntime::Split),
        UiComponent::new("Badge", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "Breadcrumb",
            &["Root", "List", "Item", "Link", "Page", "Separator"],
            UiRuntime::Server,
        ),
        UiComponent::new("Button", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "Calendar",
            &["Root", "Day", "Month", "Next", "Previous"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Card",
            &["Root", "Header", "Title", "Description", "Body", "Footer"],
            UiRuntime::Server,
        ),
        UiComponent::new(
            "Carousel",
            &["Root", "Content", "Item", "Previous", "Next"],
            UiRuntime::Split,
        ),
        UiComponent::new("Chart", &["Root", "Tooltip", "Legend"], UiRuntime::Split),
        UiComponent::new("Checkbox", &["Root", "Indicator"], UiRuntime::Client),
        UiComponent::new(
            "Collapsible",
            &["Root", "Trigger", "Content"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Command",
            &["Root", "Input", "List", "Item", "Group", "Empty"],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "ContextMenu",
            &[
                "Root",
                "Trigger",
                "Body",
                "Item",
                "Separator",
                "Shortcut",
                "Sub",
            ],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "DataTable",
            &["Root", "Header", "Body", "Row", "Cell", "Pagination"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "DatePicker",
            &["Root", "Trigger", "Calendar"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Dialog",
            &[
                "Root",
                "Trigger",
                "Overlay",
                "Body",
                "Header",
                "Footer",
                "Title",
                "Description",
                "Close",
            ],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Drawer",
            &[
                "Root", "Trigger", "Overlay", "Body", "Header", "Footer", "Close",
            ],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "DropdownMenu",
            &["Root", "Trigger", "Body", "Item", "Separator", "Shortcut"],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "Form",
            &["Root", "Field", "Label", "Control", "Message", "Submit"],
            UiRuntime::Split,
        )
        .with_form(FormContract::validator_backed()),
        UiComponent::new("HoverCard", &["Root", "Trigger", "Body"], UiRuntime::Client),
        UiComponent::new("Input", &["Root"], UiRuntime::Client),
        UiComponent::new(
            "InputOtp",
            &["Root", "Group", "Slot", "Separator"],
            UiRuntime::Client,
        ),
        UiComponent::new("Label", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "Menubar",
            &["Root", "Menu", "Trigger", "Body", "Item"],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "NavigationMenu",
            &["Root", "List", "Item", "Trigger", "Body", "Link"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Pagination",
            &["Root", "Content", "Item", "Previous", "Next"],
            UiRuntime::Server,
        ),
        UiComponent::new("Popover", &["Root", "Trigger", "Body"], UiRuntime::Client),
        UiComponent::new("Progress", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "RadioGroup",
            &["Root", "Item", "Indicator"],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "Resizable",
            &["PanelGroup", "Panel", "Handle"],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "ScrollArea",
            &["Root", "Viewport", "Scrollbar"],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Select",
            &["Root", "Trigger", "Body", "Item", "Value"],
            UiRuntime::Client,
        ),
        UiComponent::new("Separator", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "Sheet",
            &[
                "Root", "Trigger", "Overlay", "Body", "Header", "Footer", "Close",
            ],
            UiRuntime::Split,
        ),
        UiComponent::new(
            "Sidebar",
            &["Root", "Header", "Body", "Footer", "Item"],
            UiRuntime::Split,
        ),
        UiComponent::new("Skeleton", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "Slider",
            &["Root", "Track", "Range", "Thumb"],
            UiRuntime::Client,
        ),
        UiComponent::new("Sonner", &["Root", "Toast", "Action"], UiRuntime::Client),
        UiComponent::new("Switch", &["Root", "Thumb"], UiRuntime::Client),
        UiComponent::new(
            "Table",
            &["Root", "Header", "Body", "Row", "Head", "Cell", "Caption"],
            UiRuntime::Server,
        ),
        UiComponent::new(
            "Tabs",
            &["Root", "List", "Trigger", "Body"],
            UiRuntime::Split,
        ),
        UiComponent::new("Textarea", &["Root"], UiRuntime::Client),
        UiComponent::new(
            "Toast",
            &["Root", "Title", "Description", "Action", "Close"],
            UiRuntime::Client,
        ),
        UiComponent::new("Toggle", &["Root"], UiRuntime::Client),
        UiComponent::new("ToggleGroup", &["Root", "Item"], UiRuntime::Client),
        UiComponent::new("Tooltip", &["Root", "Trigger", "Body"], UiRuntime::Client),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_test_api_from_root_uf_module() {
        let root = module_by_specifier("@uniflowed/core").expect("root module");

        assert_eq!(root.kind, NativeModuleKind::Runtime);
        assert!(root.flow_exports.iter().any(|export| export == "describe"));
        assert!(root.flow_exports.iter().any(|export| export == "it"));
    }

    #[test]
    fn includes_react_flow_app_builtins() {
        let modules = builtin_modules();
        let specs = modules
            .iter()
            .map(|module| module.specifier.as_str())
            .collect::<Vec<_>>();

        assert!(specs.contains(&"@uniflowed/router"));
        assert!(specs.contains(&"@uniflowed/react"));
        assert!(specs.contains(&"@uniflowed/react-native"));
        assert!(specs.contains(&"@uniflowed/testing"));
        assert!(specs.contains(&"@uniflowed/lib"));
        assert!(specs.contains(&"@uniflowed/lint"));
        assert!(specs.contains(&"@uniflowed/server"));
        assert!(specs.contains(&"@uniflowed/hooks"));
        assert!(specs.contains(&"@uniflowed/query"));
        assert!(specs.contains(&"@uniflowed/fetch"));
        assert!(specs.contains(&"@uniflowed/loader"));
        assert!(specs.contains(&"@uniflowed/effect"));
        assert!(specs.contains(&"@uniflowed/relay"));
        assert!(specs.contains(&"@uniflowed/graphql"));
        assert!(specs.contains(&"@uniflowed/web"));
        assert!(specs.contains(&"@uniflowed/markdown"));
        assert!(specs.contains(&"@uniflowed/temporal"));
        assert!(specs.contains(&"@uniflowed/pwa"));
        assert!(specs.contains(&"@uniflowed/prepare"));
        assert!(specs.contains(&"@uniflowed/stylex"));
        assert!(specs.contains(&"@uniflowed/ui"));
        assert!(specs.contains(&"@uniflowed/react-compiler"));
        assert!(specs.contains(&"@uniflowed/flow-cell"));
        assert!(specs.contains(&"@uniflowed/state"));
        assert!(specs.contains(&"@uniflowed/validator"));
        assert!(specs.contains(&"@uniflowed/mock"));
        assert!(specs.contains(&"@uniflowed/browser"));
        assert!(specs.contains(&"@uniflowed/story"));
        assert!(specs.contains(&"@uniflowed/vrt"));
        assert!(specs.contains(&"@uniflowed/motion"));
        assert!(specs.contains(&"@uniflowed/tui"));
        assert!(specs.contains(&"@uniflowed/cli"));
    }

    #[test]
    fn tui_contract_targets_opentui_and_react_ink_replacement() {
        let module = module_by_specifier("@uniflowed/tui").expect("tui module");
        let contract = tui_contract();

        assert_eq!(module.kind, NativeModuleKind::Ui);
        assert!(
            module
                .flow_exports
                .iter()
                .any(|export| export == "renderTui")
        );
        assert!(contract.react_ink_target.replacement_ready);
        assert!(contract.has_component("FrameBuffer"));
        assert!(contract.has_component("EmbeddedTerminal"));
    }

    #[test]
    fn ui_registry_uses_compound_parts_for_complex_components() {
        let dialog = ui_components()
            .into_iter()
            .find(|component| component.name == "Dialog")
            .expect("Dialog");

        assert_eq!(dialog.runtime, UiRuntime::Split);
        assert!(dialog.preset_style);
        assert!(dialog.has_part("Body"));
        assert!(dialog.has_part("Trigger"));
    }

    #[test]
    fn ui_registry_covers_shadcn_style_catalog() {
        let components = ui_components();
        let names = components
            .iter()
            .map(|component| component.name.as_str())
            .collect::<Vec<_>>();

        assert!(names.contains(&"Accordion"));
        assert!(names.contains(&"Command"));
        assert!(names.contains(&"DataTable"));
        assert!(names.contains(&"Sheet"));
        assert!(names.contains(&"Tooltip"));
        assert!(components.len() >= 40);
    }

    #[test]
    fn form_registry_is_validator_backed_and_react_compiler_safe() {
        let form = ui_components()
            .into_iter()
            .find(|component| component.name == "Form")
            .expect("Form");
        let contract = form.form.expect("form contract");

        assert_eq!(form.runtime, UiRuntime::Split);
        assert_eq!(contract.validator_module, "@uniflowed/validator");
        assert_eq!(contract.schema_kind, SchemaKind::Object);
        assert!(contract.compiler_safe);
        assert!(contract.render_idempotent);
        assert_eq!(
            contract.mutation_phase,
            FormMutationPhase::EventOrServerAction
        );
    }

    #[test]
    fn hooks_registry_prefers_react_idempotency() {
        let hooks = hook_descriptors();

        assert!(hooks.iter().all(|hook| hook.idempotent_render));
        assert!(hooks.iter().any(|hook| hook.name == "useStableCallback"));
        assert!(hooks.iter().any(|hook| hook.server_component_safe));
    }
}
