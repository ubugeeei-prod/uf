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
        // Implemented in `packages/ui/alert-dialog.js`: `Dialog` with the three
        // decisions that module names taken the other way — `alertdialog`
        // rather than `dialog`, no dismissal from a press outside, and focus on
        // the least destructive action. `Overlay` beyond the first guess,
        // because an alert dialog has a backdrop like any other modal, and
        // `Description` is required rather than optional: the role exists to
        // announce one, so an alert dialog without it interrupts the reader to
        // say nothing.
        UiComponent::new(
            "AlertDialog",
            &[
                "Root",
                "Trigger",
                "Overlay",
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
        // Implemented in `packages/ui/calendar.js`. `Month` is the grid and its
        // caption rather than a wrapper around several months: one month is one
        // `role="grid"` with one caption naming it, and a component that put two
        // grids under one caption would have nothing to name either of them.
        //
        // There is no `Status` part, which every other component with a live
        // region has. A reader has no reason to place the sentence that says the
        // month changed, and `Combobox.Status`'s constraint - the region has to
        // be in the document *before* the text arrives, or it is not announced -
        // is easier to keep when the component owns it than when the caller can
        // forget it.
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
        // Implemented in `packages/ui/carousel.js`. `Pause` beyond the first
        // guess, and it is the part the component exists for: WCAG 2.2.2
        // requires a mechanism to stop anything that moves by itself for more
        // than five seconds, and the APG puts that control *first* inside the
        // carousel — so it is a named part rather than something `Root`
        // conjures, and `Root` raises when an autoplaying carousel has not been
        // given one.
        UiComponent::new(
            "Carousel",
            &["Root", "Content", "Item", "Pause", "Previous", "Next"],
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
                "Root",
                "Label",
                "Input",
                "List",
                "Option",
                "Group",
                "GroupLabel",
                "Empty",
                "Status",
            ],
            UiRuntime::Client,
        ),
        // Deliberately not implemented, and listed so that the decision is
        // visible where somebody would look for the component rather than only
        // in a documentation page. A command palette is a `Combobox` in a
        // `Dialog` — every part below already exists — so a seventh module
        // would be a second spelling of two that are already there, and one
        // more surface to keep in step with both.
        //
        // The one thing it would genuinely add is a palette with no results
        // that still traps focus, which is a `Dialog` question rather than a
        // `Command` one. `docs/app/reference/ui` shows the composition.
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
        // Deliberately not implemented, which is the same answer shadcn gives:
        // it ships a guide rather than a component, because a data table is a
        // table-state library composed with a table. uf has no TanStack Table
        // equivalent — `@uniflowed/query` is the fetching layer, not table
        // state — so a `DataTable` here would mean shipping that library
        // first. `packages/ui/table.js` is the half that is uf's to own: the
        // accessibility of a table whose state somebody else holds.
        UiComponent::new(
            "DataTable",
            &["Root", "Header", "Body", "Row", "Cell", "Pagination"],
            UiRuntime::Split,
        ),
        // Implemented in `packages/ui/date-picker.js`: `Popover` and `Calendar`
        // composed, plus the three joins between them.
        //
        // `Input` beyond the first guess, and it is the part the component
        // exists for. A date picker whose only input is the grid is slower for
        // everybody who already knows the date and unusable for anybody who
        // cannot operate a grid, so the field is the control and the calendar is
        // the second way in - which makes the field a named part rather than
        // something the caller is left to wire up beside one.
        UiComponent::new(
            "DatePicker",
            &["Root", "Input", "Trigger", "Calendar"],
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
        // Implemented in `packages/ui/drawer.js`, and rendered by `sheet.js`
        // rather than written a second time: a drawer is a sheet plus a
        // gesture. `Handle` is that gesture, and it is a `role="slider"` over
        // the snap points because WCAG 2.1.1 wants every one of them reachable
        // from the keyboard and WCAG 2.5.7 wants everything a drag achieves
        // achievable without one. `Title` and `Description` because a drawer is
        // a dialog and a dialog needs a name.
        UiComponent::new(
            "Drawer",
            &[
                "Root",
                "Trigger",
                "Overlay",
                "Body",
                "Handle",
                "Header",
                "Footer",
                "Title",
                "Description",
                "Close",
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
        // Implemented in `packages/ui/hover-card.js`: the third of the anchored
        // overlays, and the one that is neither of the other two. It is not a
        // tooltip, because its contents are links and a tooltip may hold
        // nothing reachable; it is not a dialog, because nothing about it is
        // modal and announcing one would be a widget the reader never asked
        // for. So it carries no role of its own and earns its keep with the
        // clause both of the others share: the pointer, and `Tab`, can get to
        // it before it closes.
        UiComponent::new("HoverCard", &["Root", "Trigger", "Body"], UiRuntime::Client),
        UiComponent::new("Input", &["Root"], UiRuntime::Client),
        // Implemented in `packages/ui/input-otp.js`, and the parts say the
        // decision the component is: `Slot` draws a character and is
        // `aria-hidden`, because there is exactly one real `<input>` underneath
        // all of them. Six `<input maxlength="1">`es lose
        // `autocomplete="one-time-code"`, the platform's SMS autofill, the
        // password manager and the single accessible name, and they lose them
        // silently.
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
        // Implemented in `packages/ui/pagination.js`: a named `<nav>`, one
        // `aria-current="page"`, previous and next named in words rather than
        // in chevrons, and a live region that was already there to say the
        // page changed.
        UiComponent::new(
            "Pagination",
            &["Root", "Content", "Item", "Previous", "Next"],
            UiRuntime::Server,
        ),
        // Implemented in `packages/ui/popover.js`, and deliberately not a
        // `Dialog` with a flag: a popover is `role="dialog"` with no
        // `aria-modal`, nothing inert, no scroll lock and — the load-bearing
        // one — no focus trap, because `Tab` leaving is how a reader gets out
        // of a popover and is the thing a trap exists to prevent. A flag would
        // mean every one of those behaviours reading it, and the one that
        // forgot would announce a modal that is not one.
        UiComponent::new("Popover", &["Root", "Trigger", "Body"], UiRuntime::Client),
        // Implemented in `packages/ui/progress.js`, and one element: the whole
        // component is the conditional that omits `aria-valuenow` when the
        // amount is unknown rather than setting it to zero.
        UiComponent::new("Progress", &["Root"], UiRuntime::Server),
        // Implemented in `packages/ui/radio-group.js`: the WAI-ARIA radio group,
        // with the arrow keys that check as they move and the tab stop an
        // unanswered group would otherwise not have.
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
        // Implemented in `packages/ui/scroll-area.js`. `Viewport` is the part
        // that earns the component: a custom scrollbar removes keyboard
        // scrolling, because a scroll container is focusable in Firefox and not
        // in Chromium, so the viewport carries `role="region"`, a name and
        // `tabindex="0"` and then intercepts no key at all. `Scrollbar` is
        // `aria-hidden` and holds no controls, which is what keeps it clear of
        // WCAG 2.5.7: nothing here is achievable only by dragging.
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
        // Implemented in `packages/ui/sheet.js`, and deliberately small: a
        // sheet is a modal dialog attached to an edge, and every modal promise
        // is `dialog.js`'s. What it adds is the two things a class name cannot
        // be — `side` as a checked union, and `data-side` as one contract that
        // `drawer.js` and `sidebar.js` are both defined against, so a drawer
        // and a sidebar-on-a-phone cannot come to disagree about what "left"
        // means. `Title` and `Description` beyond the first guess, because a
        // modal without a name is announced as "dialog".
        UiComponent::new(
            "Sheet",
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
        // Implemented in `packages/ui/sidebar.js`, and the one of the four
        // built on `Dialog` that is usually not a dialog: it is part of the
        // page, nothing behind it is inert, and there is no focus trap — until
        // the viewport gets narrow, when it becomes a `Sheet` and every one of
        // those changes. `Trigger` beyond the first guess, because the
        // `aria-expanded` that says whether the navigation is showing is the
        // component's to own; `Body` is a named `<nav>` landmark rather than a
        // `<div>`, and `Item` takes a `label` that stays the accessible name
        // once the sidebar has collapsed to icons.
        UiComponent::new(
            "Sidebar",
            &["Root", "Trigger", "Header", "Body", "Footer", "Item"],
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
        // Implemented in `packages/ui/table.js`. A real `<table>` and
        // deliberately not a `role="grid"`: a grid is a two-dimensional
        // keyboard contract rather than an attribute, and declaring one
        // without it takes the arrow keys away from the screen reader's own
        // table-reading commands and gives nothing back.
        //
        // `RowHeader`, `SelectAll` and `RowSelect` beyond the table's first
        // guess, and `Client` rather than `Server`, because the parts that
        // earn the component are the sort state, the announcement and the
        // `mixed` "select all" — none of which a server can hold.
        UiComponent::new(
            "Table",
            &[
                "Root",
                "Caption",
                "Header",
                "Body",
                "Row",
                "Head",
                "RowHeader",
                "Cell",
                "SelectAll",
                "RowSelect",
            ],
            UiRuntime::Client,
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
        // Implemented in `packages/ui/tooltip.js`. `Provider` beyond the three
        // parts a tooltip needs, and it is not decoration: it holds the clock a
        // group of tooltips shares, so the second icon in a toolbar opens at
        // once rather than making a reader who has already waited out the delay
        // wait it out again. A tooltip outside one is still a complete tooltip
        // with a delay of its own.
        UiComponent::new(
            "Tooltip",
            &["Provider", "Root", "Trigger", "Body"],
            UiRuntime::Client,
        ),
    ]
}
