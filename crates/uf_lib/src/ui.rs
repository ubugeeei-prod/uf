//! The table of headless UI components and the parts each one exposes.
//!
//! Data in the same sense as the module registry: the component names, the slot
//! names they render, whether they need the client, and which of them carry a
//! validator-backed form contract.
//!
//! # What an entry claims, and what holds it to it
//!
//! Every entry carries a [`crate::UiReadiness`], and that field decides whether
//! the rest of it is a fact or a sketch. It exists because for a long time this
//! table was read as an inventory while only ever having been written as a
//! roadmap: when ubugeeei-prod/uf#249 was filed it held fifty-one names, of
//! which the package shipped seven, and `uf inspect` reported the fifty-one as
//! a project fact.
//!
//! No count is written down here now, and that is deliberate — a number in a
//! comment is the thing that went wrong. `uf inspect` counts the readinesses,
//! and the tests below hold each one to `packages/ui`, so the count is produced
//! rather than remembered.
//!
//! * **Implemented** — `packages/ui` exports it, and `parts` is exactly what
//!   its namespace object in `packages/ui/index.js` holds. Two tests in
//!   `crates/uf_lib/src/tests.rs` read the package and fail naming both
//!   directions.
//! * **Planned** — nobody has written it yet. The parts are the shape it is
//!   expected to take, which is a design note rather than a promise.
//! * **Declined** — deliberately not a component in this package. The entry
//!   stays, because a decision nobody can find where they went looking for the
//!   component gets re-made by the next contributor; the comment on each one
//!   says what was decided and what would reopen it.
//!
//! The check is ubugeeei-prod/uf#561, and the reason it had to exist is on the
//! record: `Combobox` grew `Group` and `GroupLabel` in #558 and this file went
//! on saying seven parts with every test passing. It was found by reading,
//! which is not a check.
//!
//! # The line between this table and the preset
//!
//! About twenty of the catalogue this table was copied from have no behaviour
//! at all. `Badge`, `Card`, `Button`, `Input`, `Label`, `Textarea`,
//! `AspectRatio` and the rest are, between them, a class list and a `<div>`.
//! For a library whose product *is* the styles that is coherent. uf split
//! behaviour from styling on purpose — `packages/ui/index.js` ships no styles
//! and `packages/stylex/index.js` is deliberately not a component library, and
//! both headers say so — and the split left that half of the catalogue with no
//! home. That is ubugeeei-prod/uf#298.
//!
//! The line that holds is not "styled versus headless" but **whether the thing
//! has a decision in it**:
//!
//! * an ARIA decision, a state machine or a keyboard requirement makes it a
//!   component here, even when it renders a single element. `Progress` is one
//!   `<div>` and it belongs, because the conditional that omits
//!   `aria-valuenow` when the amount is unknown, rather than setting it to
//!   zero, *is* the component.
//! * a class list makes it a preset function, because
//!   `buttonStyles({ tone, size })` returns `{ className }` for a caller to
//!   spread onto their own element, and a `<Badge>` that renders
//!   `<span>{children}</span>` is a worse answer to the same question — it
//!   takes the element away and gives nothing back.
//! * so the second group stays here `Declined`, with `preset_styles` naming
//!   the functions that are the answer instead. Stating it is the point:
//!   left as an omission it reads as an oversight, and somebody adds the
//!   `<Badge>`.
//!
//! Five of that twenty are in the first group and ship — `Alert`, `Avatar`,
//! `Breadcrumb`, `Separator` and `Skeleton` — each for one specific reason its
//! entry gives, and each one or two elements. They were the whole of the
//! `Planned` half, so nothing here is planned today: every entry is either a
//! component a caller can import or a decision not to have one. That is a state
//! this table is allowed to be in and not one it should be assumed to stay in —
//! a new name arrives `Planned`, and the readiness is what keeps it from being
//! read as an inventory in the meantime.

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
        // Implemented in `packages/ui/alert.js`, and the one of the
        // presentational twenty whose usual shape is arguably wrong to copy.
        // `role="alert"` is a live region: an element that is *already in the
        // document* when the page loads announces on insertion or not at all,
        // so a permanently rendered "your trial ends soon" box with that role is
        // either an interruption on every load or silence. So the role is behind
        // `live`, a static callout gets none, and there is no polite option
        // because a polite region has to have been there first — which is
        // `Toast`. `packages/ui/field.js` makes the same call for
        // `Field.Error`. `Title` is a real heading whose level is the caller's,
        // for the reason `Accordion.Header` gives.
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
        )
        .styled_by(&["backdropStyles", "dialogStyles"]),
        // Declined, and for the opposite reason to the rest of them: the CSS
        // `aspect-ratio` property has done this in every browser uf supports for
        // years. There is no decision left for a component to make and no
        // element it could render that a caller could not, so the honest answer
        // is a line of documentation. Reopens if a layout uf ships ever needs a
        // ratio held in JavaScript, which no part of this package does.
        UiComponent::new("AspectRatio", &["Root"], UiRuntime::Server).declined(),
        // Implemented in `packages/ui/avatar.js`, and a component rather than a
        // class list because it is a three-state machine: loading, loaded,
        // failed, with the fallback held back briefly so it does not flash
        // before a cached image paints. The image is asked whether it has
        // painted rather than only listened to, because a cached one can finish
        // before React attaches `onLoad`; the question is asked in one direction
        // only, since "complete with no pixels" is also what a DOM that does not
        // fetch images says about a perfectly good source. The second decision
        // is the alt text — an avatar beside the person's name is decorative and
        // takes `alt=""`, and a component that puts the name there by default
        // makes every screen reader say it twice.
        UiComponent::new("Avatar", &["Root", "Image", "Fallback"], UiRuntime::Split),
        // Declined: a badge with no styles is a `<span>`. Nothing about it is a
        // decision — no role, no state, no key — so shipping one from a package
        // that ships no styles would be shipping an empty element. It has no
        // `preset_styles` yet either, and that is a gap in the preset rather
        // than in this table: the thing to add is a `badgeStyles`, not a
        // `<Badge>`.
        UiComponent::new("Badge", &["Root"], UiRuntime::Server).declined(),
        // Implemented in `packages/ui/breadcrumb.js`, and three decisions deep
        // rather than presentational: a `<nav aria-label="Breadcrumb">` around
        // an ordered list, `aria-current="page"` carried by `Page` so the last
        // crumb cannot be a link by accident, and the "/" separators
        // `aria-hidden` so the trail is not read as "Home slash Settings slash
        // Billing". `Pagination` below is the same shape, and `List` states the
        // same `renders*` constraint about what an `<ol>` may hold.
        UiComponent::new(
            "Breadcrumb",
            &["Root", "List", "Item", "Link", "Page", "Separator"],
            UiRuntime::Server,
        ),
        // Declined, and the clearest case of the line this table draws: the
        // platform's `<button>` already has the role, the keyboard and the
        // focus, so what shadcn's `Button` adds is a class list. The answer is
        // `buttonStyles({ tone, size })` on the caller's own `<button>`, which
        // keeps `type="submit"`, a `formAction` and everything else an element
        // wrapped in a component quietly loses.
        UiComponent::new("Button", &["Root"], UiRuntime::Server)
            .declined()
            .styled_by(&["buttonStyles"]),
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
        // Declined: six parts, and every one of them is a `<div>` with a class
        // on it. A card has no role — `role="region"` needs a name, and naming
        // every card on a page is how a landmark list becomes useless — so the
        // heading a caller writes is the whole of its semantics.
        UiComponent::new(
            "Card",
            &["Root", "Header", "Title", "Description", "Body", "Footer"],
            UiRuntime::Server,
        )
        .declined()
        .styled_by(&["surfaceStyles", "cardStyles"]),
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
        // Declined, and the one whose reason is a product decision rather than
        // an accessibility one: shadcn's `Chart` is a Recharts wrapper, and uf
        // has no charting library to wrap. Shipping one would mean taking on a
        // whole dependency, which is a decision about what this project is
        // rather than a component gap. Reopens the day somebody makes it, and
        // the entry stays so that the day is a decision rather than a surprise.
        UiComponent::new("Chart", &["Root", "Tooltip", "Legend"], UiRuntime::Split).declined(),
        // Implemented in `packages/ui/checkbox.js` as one component with no
        // namespace, which is why there is one part and not the `Root` and
        // `Indicator` this entry used to claim. That was not a typo and the
        // answer is not to grow the part: an indicator is a tick, a tick is a
        // drawing, and a package that ships no styles has nothing to draw. A
        // caller renders their own mark against `data-state`, and
        // `controlStyles({ shape: "box" })` is the preset's.
        UiComponent::new("Checkbox", &["Root"], UiRuntime::Client).styled_by(&["controlStyles"]),
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
        )
        .declined(),
        // Implemented in `packages/ui/context-menu.js`, which is `menu.js` with
        // the two things that make a context menu a component rather than an
        // `oncontextmenu` handler: it opens at the *pointer* through the
        // positioner's virtual anchor, and it opens from the keyboard —
        // `Shift+F10` and the `ContextMenu` key — because a command reachable
        // only by right-click is a WCAG 2.1.1 failure. Long press is the touch
        // spelling of the same gesture.
        //
        // There is no `Shortcut`: the keystroke drawn beside a command is a
        // `<span>` with no behaviour and no ARIA of its own, which is a style
        // rather than a part. The parts below are `Menu`'s, because the body of
        // a context menu *is* a menu.
        UiComponent::new(
            "ContextMenu",
            &[
                "Root",
                "Trigger",
                "Body",
                "Item",
                "CheckboxItem",
                "RadioGroup",
                "RadioItem",
                "Separator",
                "Group",
                "Label",
                "Sub",
                "SubTrigger",
            ],
            UiRuntime::Client,
        )
        .styled_by(&["menuStyles", "menuItemStyles"]),
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
        )
        .declined(),
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
        )
        .styled_by(&["backdropStyles", "dialogStyles"]),
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
        // Implemented in `packages/ui/field.js`, and listed here for the first
        // time: it shipped with #276 and this table never learned about it,
        // while carrying a `Form` entry for a component that does not exist.
        // That pair — a real module the table did not know about beside an
        // imagined one it asserted a contract for — is ubugeeei-prod/uf#249 in
        // one line, and the reason the checks in `crates/uf_lib/src/tests.rs`
        // now read the package rather than trusting this file.
        //
        // `Control` is a render function rather than an element, because a field
        // wraps a select, a textarea or somebody else's component as often as an
        // input, and each needs the same attributes on whatever it renders.
        // There is no `Group` part: a group is `Field.Root group`, because what
        // changes is the root's role and its label's element rather than
        // anything a new part would render.
        UiComponent::new(
            "Field",
            &["Root", "Label", "Control", "Description", "Status", "Error"],
            UiRuntime::Client,
        )
        .styled_by(&["fieldStyles"]),
        // Declined. `Root`, `Field`, `Label`, `Control`, `Message` and `Submit`
        // is shadcn's Form over react-hook-form, and uf does not ship a
        // component by that name: the same job is `@uniflowed/form` for the
        // state and `Field` above for the markup, joined by `useFieldSource` —
        // which is where it has to live, because `@uniflowed/ui` is published
        // and `@uniflowed/form` is not, and `tools/ci/publishable.sh` refuses
        // that dependency. The contract below is about that pair, and it stays
        // on this entry rather than moving to `Field`: it is a claim about
        // where a form's validation lives, and `Field` renders markup for a
        // value it does not own.
        //
        // This is the entry ubugeeei-prod/uf#249 names. The contract asserted
        // that a `Form` was validator-backed, React Compiler-safe and mutated
        // only in an event or a Server Action, and there was no `Form` to be
        // any of those things. The readiness is what now says so.
        UiComponent::new(
            "Form",
            &["Root", "Field", "Label", "Control", "Message", "Submit"],
            UiRuntime::Split,
        )
        .declined()
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
        // Declined: an `<input>` already is the component. Wrapping one costs a
        // caller `type`, `inputMode`, `autoComplete` and the ref, and buys a
        // class name. `Field` above is the part that is genuinely uf's — the
        // label, the description, the error and the one `aria-describedby` that
        // joins them — and `Field.Control` hands the props to the caller's own
        // `<input>` rather than rendering one.
        UiComponent::new("Input", &["Root"], UiRuntime::Client)
            .declined()
            .styled_by(&["fieldStyles"]),
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
        // Declined twice over: `<label htmlFor>` is the platform's, and the
        // part of labelling that is hard — a control whose id the caller never
        // has to write, and a label that is a `<label>` for one control and a
        // `<span>` for a group — is `Field.Label`, which exists. What is left
        // is the type scale.
        UiComponent::new("Label", &["Root"], UiRuntime::Server)
            .declined()
            .styled_by(&["textStyles"]),
        // Implemented in `packages/ui/menu.js`, and the base the menu-shaped
        // components above and below it are built from.
        //
        // **This is shadcn's Dropdown Menu.** There is no fourth entry and no
        // alias export: a dropdown menu is a menu whose trigger is a button,
        // which is what `Trigger` is, and a second name for one component is a
        // second surface to keep in step. `ContextMenu` and `Menubar` are the
        // two that genuinely differ, and each says how in its own comment.
        //
        // `CheckboxItem`, `RadioGroup` and `RadioItem` are the checkable kinds
        // all three menus share. `ITEM_SELECTOR` named their roles before there
        // was a component that rendered them, so the arrow keys and the
        // typeahead already stepped across them.
        UiComponent::new(
            "Menu",
            &[
                "Root",
                "Trigger",
                "Body",
                "Item",
                "CheckboxItem",
                "RadioGroup",
                "RadioItem",
                "Separator",
                "Group",
                "Label",
                "Sub",
                "SubTrigger",
            ],
            UiRuntime::Client,
        )
        .styled_by(&["menuStyles", "menuItemStyles"]),
        // Implemented in `packages/ui/menubar.js`: a row of `Menu`s with a
        // keyboard map the set has and a single menu does not. One tab stop for
        // the whole bar, arrows between the top-level menus, and — the part
        // that is always missing — arrows *while a menu is open* that close it
        // and open the adjacent one, so File → Edit → View needs no `Escape`
        // between them. `Menu` is the value of the bar's `open`, which is why
        // it is a part rather than a nesting convention.
        UiComponent::new(
            "Menubar",
            &[
                "Root",
                "Menu",
                "Trigger",
                "Body",
                "Item",
                "CheckboxItem",
                "RadioGroup",
                "RadioItem",
                "Separator",
                "Group",
                "Label",
                "Sub",
                "SubTrigger",
            ],
            UiRuntime::Client,
        )
        .styled_by(&["menuStyles", "menuItemStyles"]),
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
        // Implemented in `packages/ui/separator.js`, and two lines with one real
        // decision in them: a separator between groups of content is
        // `role="separator"` with an `aria-orientation`, and a decorative rule
        // is `aria-hidden` and announced to nobody. The default is the semantic
        // one, because a rule wrongly announced is noise a reader can skip and a
        // boundary wrongly silent is information nobody finds out about. A
        // `<div>` rather than an `<hr>`, which arrives with a border and a
        // margin from the browser's own stylesheet and is horizontal by
        // definition. `packages/ui/menu.js` ships the in-menu one; not
        // `Resizable.Handle`, which is the APG window splitter — a separator
        // that behaves like a slider — and says so in its own entry.
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
        // Implemented in `packages/ui/skeleton.js`, and the one that silently
        // makes a page worse: a screen of skeletons is a screen of empty boxes,
        // so the reader hears nothing and is told nothing is happening. `Box` is
        // `aria-hidden="true"`, the `Root` region is `aria-busy="true"`, and a
        // live region says "Loading" out loud. Two parts rather than the one
        // this entry guessed at, because the region and the grey box are
        // opposite halves of the same fix — and `Split` rather than `Server`,
        // because the region is mounted empty and filled a commit later, which
        // is the only way a screen that is busy on its first render is ever
        // announced at all.
        UiComponent::new("Skeleton", &["Root", "Box"], UiRuntime::Split),
        // Implemented in `packages/ui/slider.js`: the APG slider, with
        // `role="slider"` on the thumb rather than on the track — which is what
        // makes it reachable — and a second thumb for the range case, each with
        // its own bounds.
        UiComponent::new(
            "Slider",
            &["Root", "Track", "Range", "Thumb"],
            UiRuntime::Client,
        ),
        // Implemented in `packages/ui/switch.js` as one component with no
        // namespace, for the reason `Checkbox` above gives at length: a `Thumb`
        // is a drawing, and this package draws nothing. `data-state` is what a
        // caller moves their own thumb against, and
        // `controlStyles({ shape: "track" })` is the preset's.
        UiComponent::new("Switch", &["Root"], UiRuntime::Client).styled_by(&["controlStyles"]),
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
        UiComponent::new("Tabs", &["Root", "List", "Tab", "Panel"], UiRuntime::Split)
            .styled_by(&["tabListStyles", "tabStyles"]),
        // Declined, for `Input`'s reasons exactly. The one thing a `Textarea`
        // component is sometimes written for — growing with its content — is a
        // `field-sizing: content` away in CSS and belongs to the preset if it
        // belongs anywhere.
        UiComponent::new("Textarea", &["Root"], UiRuntime::Client)
            .declined()
            .styled_by(&["fieldStyles"]),
        // Implemented in `packages/ui/toast.js`. `Region` is the part the table
        // was missing and the one the component exists for: the live region has
        // to be in the document before the notification it announces, so it is
        // a part a caller renders once in the layout rather than something a
        // `Root` conjures when a message arrives.
        //
        // **`Sonner` was a separate entry below and is gone.** It was this
        // component under another project's name, and ubugeeei-prod/uf#249 asked
        // for that to be settled: a second name is a second surface to keep in
        // step, and it is the shape of claim this table was full of — an entry
        // for something no import could reach.
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
