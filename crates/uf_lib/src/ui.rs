//! The table of headless UI components and the parts each one exposes.
//!
//! Data in the same sense as the module registry: the component names, the slot
//! names they render, whether they need the client, and which of them carry a
//! validator-backed form contract.

use crate::descriptor::{FormContract, UiComponent, UiRuntime};

pub fn ui_components() -> Vec<UiComponent> {
    vec![
        // Implemented in `packages/ui/accordion.js`. `Header` is a part rather
        // than something the trigger renders for itself, because the heading
        // level belongs to the caller: an accordion inside an `<h2>` section
        // needs `<h3>`, and a hard-coded level produces a document outline
        // nobody can navigate.
        UiComponent::new(
            "Accordion",
            &["Root", "Item", "Header", "Trigger", "Content"],
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
        // Implemented in `packages/ui/collapsible.js`: the WAI-ARIA disclosure
        // pattern on its own, and the module the accordion's argument about a
        // closed panel staying findable is written against.
        UiComponent::new(
            "Collapsible",
            &["Root", "Trigger", "Content"],
            UiRuntime::Split,
        ),
        // Implemented in `packages/ui/combobox.js`: the ARIA 1.2 combobox with
        // `aria-activedescendant` over a caller-filtered listbox.
        UiComponent::new(
            "Combobox",
            &[
                "Root", "Label", "Input", "List", "Option", "Empty", "Status",
            ],
            UiRuntime::Client,
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
        // Implemented in `packages/ui/menu.js`, and the base the menu-shaped
        // components above and below it are built from.
        UiComponent::new(
            "Menu",
            &[
                "Root",
                "Trigger",
                "Body",
                "Item",
                "Separator",
                "Group",
                "Label",
                "Sub",
                "SubTrigger",
            ],
            UiRuntime::Client,
        ),
        UiComponent::new(
            "Menubar",
            &["Root", "Menu", "Trigger", "Body", "Item"],
            UiRuntime::Client,
        ),
        // Implemented in `packages/ui/navigation-menu.js`, and deliberately not
        // one of the menus above: it is the practices' *Disclosure Navigation
        // Menu* — a `<nav>` of links behind `aria-expanded` buttons — because
        // `menu`/`menuitem` are for application commands, and a reader told
        // "menu, five items" expected a list of links.
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
        // Implemented in `packages/ui/radio-group.js`: the WAI-ARIA radio group,
        // with the arrow keys that check as they move and the tab stop an
        // unanswered group would otherwise not have.
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
        // `Tab` and `Panel` rather than `Trigger` and `Body`: the shipped parts
        // are named after the ARIA roles they render, so `Tabs.Tab` is a `tab`
        // and `Tabs.Panel` is a `tabpanel`. See `packages/ui/tabs.js`.
        UiComponent::new("Tabs", &["Root", "List", "Tab", "Panel"], UiRuntime::Split),
        UiComponent::new("Textarea", &["Root"], UiRuntime::Client),
        UiComponent::new(
            "Toast",
            &["Root", "Title", "Description", "Action", "Close"],
            UiRuntime::Client,
        ),
        // Implemented in `packages/ui/toggle.js`, and the shipped part is the
        // component itself rather than a `Toggle.Root`: there is nothing to
        // compose, exactly as with `Switch` and `Checkbox`, the other two
        // two-state controls it is deliberately not either of.
        UiComponent::new("Toggle", &["Root"], UiRuntime::Client),
        // Implemented in `packages/ui/toggle-group.js`. `type="multiple"` is a
        // `group` of `aria-pressed` buttons; `type="single"` is the radio group
        // pattern drawn as segments, and is rendered by `radio-group.js` rather
        // than written a second time.
        UiComponent::new("ToggleGroup", &["Root", "Item"], UiRuntime::Client),
        UiComponent::new("Tooltip", &["Root", "Trigger", "Body"], UiRuntime::Client),
    ]
}
