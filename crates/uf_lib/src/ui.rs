//! The table of headless UI components and the parts each one exposes.
//!
//! Data in the same sense as the module registry: the component names, the slot
//! names they render, whether they need the client, and which of them carry a
//! validator-backed form contract.

use crate::descriptor::{FormContract, UiComponent, UiRuntime};

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
        // Implemented in `packages/ui/progress.js`, and one element: the whole
        // component is the conditional that omits `aria-valuenow` when the
        // amount is unknown rather than setting it to zero.
        UiComponent::new("Progress", &["Root"], UiRuntime::Server),
        UiComponent::new(
            "RadioGroup",
            &["Root", "Item", "Indicator"],
            UiRuntime::Client,
        ),
        // Implemented in `packages/ui/resizable.js`: the APG window splitter,
        // which is a separator that behaves like a slider. Not the same
        // `separator` as `Menu`'s — that one is a rule between groups and is
        // not focusable — and the two module headers each say which they are.
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
        // Implemented in `packages/ui/select.js`: the ARIA 1.2 *select-only*
        // combobox, the other half of the pattern `Combobox` implements.
        //
        // `List` and `Option` rather than `Body` and `Item`, for the reason
        // `Tabs` gives below — the shipped parts are named after the ARIA roles
        // they render — and because a reader who has met `Combobox.List` and
        // `Combobox.Option` should not have to learn two names for the same
        // listbox. `Label` names the field and `GroupLabel` names a group of
        // options; shadcn has one `SelectLabel` and it is the second of those.
        UiComponent::new(
            "Select",
            &[
                "Root",
                "Label",
                "Trigger",
                "Value",
                "List",
                "Option",
                "Group",
                "GroupLabel",
                "Separator",
            ],
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
        // Implemented in `packages/ui/slider.js`: the APG slider, with
        // `role="slider"` on the thumb rather than on the track — which is what
        // makes it reachable — and a second thumb for the range case, each with
        // its own bounds.
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
        // Implemented in `packages/ui/toast.js`. `Region` is the part the table
        // was missing and the one the component exists for: the live region has
        // to be in the document before the notification it announces, so it is
        // a part a caller renders once in the layout rather than something a
        // `Root` conjures when a message arrives.
        //
        // `Sonner` below is the same component under another project's name,
        // which is ubugeeei-prod/uf#249's to settle rather than this entry's.
        UiComponent::new(
            "Toast",
            &["Region", "Root", "Title", "Description", "Action", "Close"],
            UiRuntime::Client,
        ),
        UiComponent::new("Toggle", &["Root"], UiRuntime::Client),
        UiComponent::new("ToggleGroup", &["Root", "Item"], UiRuntime::Client),
        UiComponent::new("Tooltip", &["Root", "Trigger", "Body"], UiRuntime::Client),
    ]
}
