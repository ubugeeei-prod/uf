// @flow
//
// A select: a button that opens a list of options and takes one of them.
//
// This is the *select-only* combobox of ARIA 1.2, and `combobox.js` is the
// editable one. They are the two halves of the same pattern and they are two
// modules, because every key means something different in each:
//
//   |                     | `Combobox`                     | `Select`                    |
//   | ------------------- | ------------------------------ | --------------------------- |
//   | Trigger             | `<input type="text">`          | `<button>`                  |
//   | Printable keys      | edit the text; caller filters  | typeahead onto an option    |
//   | `Home` / `End`      | left to the text cursor        | first and last option       |
//   | `Enter`, nothing active | left to the form           | opens, or takes the cursor  |
//   | Value               | the text, which is not a value | the option, always          |
//
// One module with a flag would have to guess which of those a keystroke meant,
// and a component that guesses gets both wrong — the same reason `switch.js`
// and `checkbox.js` are apart.
//
// What the two do share is the focus model, and it is the part hand-written
// selects get wrong. Focus never leaves the trigger. The arrow keys move
// `aria-activedescendant`, a second cursor naming which option is current while
// the real focus stays on the button, so the reader is told the option changed.
// A highlight drawn in CSS moves the same pixels and says nothing.
//
// # Why a listbox and not a native `<select>`
//
// A `<select>` is better than this component in every way a `<select>` can be:
// it is the platform's, it is announced correctly by software this package has
// never been tested against, on a phone it is a wheel the thumb already knows,
// it autofills, and it validates. **If a native `<select>` will do, use one** —
// that is not a disclaimer, it is the recommendation, and it is why this module
// exists rather than a `Select` that renders `<select>` and calls it headless.
// Wrapping the native control would add nothing a caller cannot write in one
// line, and this package's premise is that it only ships the part that is hard.
//
// The part that is hard is what a `<select>` cannot do: its popup is drawn by
// the operating system, so an option cannot hold an icon, a second line, a
// keyboard shortcut or a checkmark, and nothing about it can be styled. A
// design that needs any of that has exactly two options — this pattern, or a
// `div` with a `click` handler that no keyboard reaches. This module is the
// first one, done properly:
//
//   * `role="combobox"` on the trigger with `aria-haspopup="listbox"`, so a
//     reader is told what the button will do before pressing it.
//   * The whole keyboard map below, including typeahead, which is the one every
//     hand-written select omits and the one that makes a list of two hundred
//     countries usable.
//   * A hidden control carrying the value, so a form submits `GB` and not
//     "United Kingdom" — `internal/form-value.js` says why it is an `<input>`.
//
// # The keyboard
//
// Closed, on the trigger:
//
//   * `Enter`, `Space`, `ArrowDown`, `ArrowUp` open the list. The cursor lands
//     on the *selected* option when there is one, not on the first: a list of
//     two hundred countries opened onto "Afghanistan" when the reader had
//     already chosen Zimbabwe is a list they have to arrow through twice.
//   * `Home` and `End` open onto the first and last option, and deliberately
//     ignore the selection — those two keys name a position, and answering a
//     question about position with the current value is not an answer.
//   * `Alt+ArrowDown` opens with no cursor at all, which is how a reader looks
//     at the options without committing to moving among them.
//   * A printable character opens the list and runs typeahead on it, so `f`
//     from a closed select reaches France in one keystroke, the way every
//     native select on every platform has always done.
//
// Open:
//
//   * `ArrowDown` / `ArrowUp` move the cursor and **do not wrap**, which is
//     where this differs from `menu.js` on purpose. A native menu cycles and a
//     native select stops, and a reader's expectation comes from the platform
//     control the widget imitates, not from the package it was shipped in.
//   * `Home` / `End` go to the ends. This is the exact inverse of
//     `combobox.js`, which leaves both keys to the text cursor, and the pair is
//     the clearest single statement of why there are two components.
//   * `Enter`, `Space` and `Alt+ArrowUp` take the option under the cursor and
//     close.
//   * `Escape` closes and changes nothing, and is stopped from travelling
//     further so a select inside a dialog does not close the dialog too.
//   * `Tab` takes the option under the cursor and moves on. This is APG's rule
//     for the select-only combobox and it is the opposite of what `Combobox`
//     does with the same key, which is worth a sentence because it looks like
//     an inconsistency and is not. In an editable combobox the reader has typed
//     something, the highlight is a suggestion about text they still own, and
//     committing it on the way out turns "leave this field" into an edit. Here
//     there is nothing typed and nothing to lose: moving the cursor *is* the
//     act of choosing, and a select that discarded it on Tab would be the only
//     select on the machine that did.
//
// # Moving the cursor does not change the value
//
// Arrowing sets `aria-activedescendant` and nothing else; the value changes on
// `Enter`, `Space`, `Alt+ArrowUp`, `Tab` and a click, and `Escape` leaves it
// alone. A native `<select>` on Windows does the opposite — the value follows
// the arrow keys — and copying it here would be a mistake with a cost the
// pattern does not have to pay. `onValueChange` is wired to a form store, a
// validation run or a server mutation, and selection-follows-focus fires all
// three once per arrow press: arrowing from the top of a country list to the
// bottom would be two hundred submissions.
//
// # Groups
//
// A `listbox` may own `option` and `group` elements, and nothing else. That
// one sentence decides three things here:
//
//   * The parts are `div`s rather than the `ul`/`li` `combobox.js` uses.
//     Nesting a group's options inside a listbox as a list means a second
//     `list` role between the group and its options, which is a child ARIA does
//     not allow the group to own.
//   * `Select.GroupLabel` is `role="presentation"` and names its group through
//     `aria-labelledby`, exactly as `Menu.Group` and `Menu.Label` do — and only
//     while a label is actually rendered, because an `aria-labelledby` naming
//     an id that is not in the document makes a reader hear nothing at all.
//   * `Select.Separator` is `aria-hidden`, which is the one place it differs
//     from `Menu.Separator`. A `separator` is a legal child of a `menu` and is
//     announced there as "the group changed"; inside a `listbox` it is not a
//     legal child, so the rule between two groups is decoration and is kept out
//     of the tree. A reader who greps this package for `separator` finds both,
//     and they are not the same thing.
//
// # What this module does not ship
//
// shadcn's Select has `ScrollUpButton` and `ScrollDownButton`. They are not
// here, and their absence is a decision rather than an omission: both exist to
// scroll a popup that Radix positions and sizes, and this package positions
// nothing and ships no styles, so a scroll button here would be a `button` with
// no idea what to scroll. The behaviour they are really for — the cursor
// staying visible as the arrow keys move it — is in this module already, as the
// `scrollIntoView({ block: "nearest" })` every move performs, and it works for
// a caller's own scroll container without either button.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Align, LogicalSide } from "./internal/anchor.js";
import { useAnchor } from "./internal/anchor.js";
import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import type { Movement } from "./internal/roving-focus.js";
import { isTypeaheadKey, itemsOf, moveTo, useTypeahead } from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";
import { FormValue } from "./internal/form-value.js";

export type { Align, LogicalSide, Side } from "./internal/anchor.js";

const OPTION_SELECTOR = '[role="option"]';
const LISTBOX_SELECTOR = '[role="listbox"]';

/**
 * Where the cursor should go once the list is in the document.
 *
 * Everything that opens the list has an opinion about where the cursor lands,
 * and none of it can be acted on yet: the options do not exist to be measured
 * until the commit that renders the listbox. So the opinion is left here for
 * `Select.List`'s effect to carry out — a ref rather than state, because
 * nothing renders it and a re-render whose only purpose is to carry a message
 * to an effect is a render nobody asked for.
 *
 * `preferSelected` is the difference between `ArrowDown`, which means "start
 * from where I am", and `End`, which means "the last one" and must not be
 * quietly answered with the current selection instead.
 */
type Landing =
  | {| readonly kind: "end", readonly end: Movement, readonly preferSelected: boolean |}
  | {| readonly kind: "typed", readonly key: string |};

type SelectState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  readonly disabled: boolean,
  /** The chosen option's value, or null when nothing is chosen. */
  readonly value: string | null,
  /** Take an option: sets the value, closes, and leaves focus on the trigger. */
  readonly choose: (value: string) => void,
  /** The id of the option `aria-activedescendant` names, if any. */
  readonly activeId: string | null,
  readonly setActiveId: (id: string | null) => void,
  readonly pendingLanding: { current: Landing | null },
  readonly triggerRef: { current: HTMLElement | null },
  readonly listRef: { current: HTMLElement | null },
  /**
   * What `Select.Value` should display for a value, learned from the options.
   *
   * See `registerLabel` for why this only ever grows.
   */
  readonly labels: { readonly [string]: string },
  readonly registerLabel: (value: string, label: string) => void,
  readonly labelled: boolean,
  readonly registerFieldLabel: (present: boolean) => void,
  /**
   * Matching by the characters a reader types.
   *
   * Held here rather than made where it is used, because there is one buffer
   * and two callers: the trigger runs it while the list is open, and
   * `Select.List`'s effect runs it for the keystroke that *opened* the list.
   * Two `useTypeahead()` calls would be two buffers, and typing "sa" fast
   * enough to be one word would be read as "s" and then "a".
   */
  readonly typeahead: (
    items: $ReadOnlyArray<HTMLElement>,
    from: number,
    key: string,
  ) => HTMLElement | null,
|};

const SelectContext: React.Context<SelectState | null> = createContext(null);

hook useSelect(part: string): SelectState {
  const state = useContext(SelectContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Select.Root`);
  }
  return state;
}

/** The id of a group's label, so `Select.Group` only claims one that exists. */
type SelectGroupState = {|
  readonly labelId: string,
  readonly registerLabel: (present: boolean) => void,
|};

const SelectGroupContext: React.Context<SelectGroupState | null> = createContext(null);

/**
 * The select.
 *
 * `name` is the only thing here a form sees. Given one, the root renders a
 * hidden control carrying the *value* — `internal/form-value.js` explains what
 * it is and why it is not a concealed `<select>`. Without one, nothing is
 * submitted, which is correct for a select that filters a table.
 *
 * A select bound to `@uniflowed/form` needs no `name`: that library keeps its
 * values in its own store and prevents the native submission, so the binding is
 * `useController` — `field.value` into `value` and `field.onChange` into
 * `onValueChange`, which is exactly the pair of props below. Neither package
 * imports the other and neither needs to.
 */
export component SelectRoot(
  children: React.Node,
  value?: string | null,
  defaultValue?: string | null = null,
  onValueChange?: (value: string | null) => void,
  open?: boolean,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  name?: string,
  disabled?: boolean = false,
  ...rest: Rest
) {
  const base = useId();
  const [chosen, setChosen] = useControlled(value, defaultValue, onValueChange);
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [labels, setLabels] = useState<{ readonly [string]: string }>({});
  const [labelled, setLabelled] = useState(false);
  const pendingLanding = useRef<Landing | null>(null);
  const triggerRef = useRef<HTMLElement | null>(null);
  const listRef = useRef<HTMLElement | null>(null);
  const typeahead = useTypeahead();

  const choose = useStableCallback((next: string) => {
    setChosen(next);
    setOpen(false);
    setActiveId(null);
    // Focus never left the trigger for a keyboard selection, and an option's
    // `pointerdown` handler stops a click taking it either — but the trigger is
    // where the next keystroke has to arrive, and asserting that here costs
    // nothing and survives a caller who renders an option as something
    // focusable.
    triggerRef.current?.focus();
  });

  /**
   * Remember what an option's value is called.
   *
   * Only ever added to, and that is the whole design. The options are in the
   * document while the list is open and gone when it is closed, which is
   * exactly when `Select.Value` needs a label to show — so forgetting on
   * unmount would blank the trigger the instant the reader chose something.
   * The map is bounded by the number of distinct values the caller has
   * rendered, which is the size of their own option list.
   */
  const registerLabel = useStableCallback((optionValue: string, label: string) => {
    setLabels((current) =>
      current[optionValue] === label ? current : { ...current, [optionValue]: label },
    );
  });

  const state = useMemo(
    () => ({
      base,
      open: isOpen,
      setOpen,
      disabled,
      value: chosen,
      choose,
      activeId,
      setActiveId,
      pendingLanding,
      triggerRef,
      listRef,
      labels,
      registerLabel,
      labelled,
      registerFieldLabel: setLabelled,
      typeahead,
    }),
    [
      base,
      isOpen,
      setOpen,
      disabled,
      chosen,
      choose,
      activeId,
      labels,
      registerLabel,
      labelled,
      typeahead,
    ],
  );

  return (
    <SelectContext.Provider value={state}>
      <div {...rest}>
        {children}
        {name == null ? null : <FormValue disabled={disabled} name={name} value={chosen} />}
      </div>
    </SelectContext.Provider>
  );
}

/**
 * The field's label.
 *
 * A `<label htmlFor>` *and* an `aria-labelledby` on the trigger, and the second
 * one is not redundant. `role="combobox"` is not a role that takes its name
 * from its own content, and a `<label for>` pointing at a `<button>` does not
 * name it either — HTML-AAM gives a button its name from its subtree, which the
 * role has just ruled out. A select with only a `<label for>` was therefore a
 * combobox with no accessible name at all, announced as "combobox" and nothing
 * else, while looking correct in the markup and reading correctly to anyone
 * who could see it. The `htmlFor` is kept for the behaviour it does carry: a
 * click on the label focuses the trigger.
 *
 * This is `Select.Label` and it names the field. `Select.GroupLabel` names a
 * group of options — the two are separate parts because a select has both, and
 * shadcn's single `SelectLabel`, which is the group's, has no name for the
 * field's.
 */
export component SelectLabel(children: React.Node, ...rest: Rest) {
  const select = useSelect("Select.Label");
  const register = select.registerFieldLabel;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <label {...rest} htmlFor={`${select.base}-trigger`} id={`${select.base}-label`}>
      {children}
    </label>
  );
}

/**
 * The button that opens the list, and every key the pattern defines.
 *
 * A `<button>` rather than the `div` with `tabindex="0"` the APG example uses,
 * for the reason `menu.js` gives about its items: focusability, `disabled` and
 * the focus ring are then the browser's rather than this component's, and a
 * component that reimplements `disabled` gets one of its four behaviours wrong.
 *
 * `type="button"` because the whole point of this module is that it lives in
 * forms, and a `<button>` inside a `<form>` submits it by default. A select
 * that posted the form every time it was opened would be a memorable bug.
 */
export component SelectTrigger(children: React.Node, ...rest: Rest) {
  const select = useSelect("Select.Trigger");
  const passed = withoutComposed(rest, ["onClick", "onKeyDown", "ref"]);

  /** The options in the document right now, in document order. */
  const options = (): Array<HTMLElement> => {
    const list = select.listRef.current;
    return list == null ? [] : itemsOf(list, OPTION_SELECTOR, LISTBOX_SELECTOR);
  };

  const put = (option: HTMLElement | null) => {
    if (option == null) {
      return;
    }
    select.setActiveId(option.id);
    // `nearest`, so a list already showing the option does not jump under a
    // reader who can see it.
    (option as $FlowFixMe).scrollIntoView?.({ block: "nearest" });
  };

  /** Move the cursor within an open list, or open with an instruction. */
  const move = (end: Movement, preferSelected: boolean) => {
    if (!select.open) {
      select.pendingLanding.current = { kind: "end", end, preferSelected };
      select.setOpen(true);
      return;
    }
    const items = options();
    const at = items.findIndex((item) => item.id === select.activeId);
    // `false`: the ends are closed. A native select stops at the last option
    // and a native menu cycles, and this widget is the first kind.
    put(moveTo(items, at, end, false));
  };

  /** Take the option under the cursor, if there is one. */
  const commit = (): boolean => {
    const option = options().find((item) => item.id === select.activeId);
    if (option == null) {
      return false;
    }
    select.choose(option.getAttribute("data-value") ?? "");
    return true;
  };

  return (
    <button
      {...passed}
      // Only while the list is in the document. Either attribute naming an
      // element that is not there makes a screen reader announce nothing where
      // it used to announce the current option.
      aria-activedescendant={select.open ? (select.activeId ?? undefined) : undefined}
      aria-controls={select.open ? `${select.base}-list` : undefined}
      aria-expanded={select.open ? "true" : "false"}
      aria-haspopup="listbox"
      // Named only while a `Select.Label` is rendered: an `aria-labelledby`
      // pointing at an id nothing has is worse than no name, because a reader
      // is told nothing rather than told the button's own content.
      aria-labelledby={select.labelled ? `${select.base}-label` : undefined}
      disabled={select.disabled}
      id={`${select.base}-trigger`}
      onClick={composeHandlers(rest.onClick, () => {
        if (select.open) {
          select.setOpen(false);
          select.setActiveId(null);
          return;
        }
        select.pendingLanding.current = { kind: "end", end: "first", preferSelected: true };
        select.setOpen(true);
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event: $FlowFixMe) => {
        if (select.disabled) {
          return;
        }

        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          if (event.altKey) {
            if (event.key === "ArrowDown") {
              // Look without moving: the list opens with no cursor at all.
              select.setOpen(true);
              return;
            }
            // `Alt+ArrowUp` is the collapse-and-take of a native select.
            if (!commit()) {
              select.setOpen(false);
              select.setActiveId(null);
            }
            return;
          }
          move(event.key === "ArrowDown" ? "next" : "previous", true);
          return;
        }

        if (event.key === "Home" || event.key === "End") {
          event.preventDefault();
          // `preferSelected` false: these two keys name a position, and
          // answering "the last one" with "the one you already chose" is not an
          // answer to the question that was asked.
          move(event.key === "Home" ? "first" : "last", false);
          return;
        }

        if (event.key === "Enter" || event.key === " ") {
          // Prevented in both branches: `Enter` would submit the form this
          // select is in, and `Space` would scroll the page and then arrive
          // again as a click.
          event.preventDefault();
          if (!select.open) {
            select.pendingLanding.current = { kind: "end", end: "first", preferSelected: true };
            select.setOpen(true);
            return;
          }
          if (!commit()) {
            // Open with nothing under the cursor: close rather than sit there,
            // which is what a reader who pressed Enter asked for.
            select.setOpen(false);
          }
          return;
        }

        if (event.key === "Escape") {
          if (!select.open) {
            return;
          }
          event.preventDefault();
          // A dialog around this select must not also close: one Escape is one
          // dismissal, and the innermost thing wins.
          event.stopPropagation();
          select.setOpen(false);
          select.setActiveId(null);
          return;
        }

        if (event.key === "Tab") {
          // Not prevented: Tab still moves on. It takes the cursor's option on
          // the way out, which is APG's rule for this pattern and the opposite
          // of `Combobox`'s — the module header says why the two differ.
          if (select.open) {
            commit();
            select.setOpen(false);
            select.setActiveId(null);
          }
          return;
        }

        if (!isTypeaheadKey(event)) {
          return;
        }
        if (!select.open) {
          event.preventDefault();
          // The options are not in the document yet, so the keystroke travels
          // to the commit that renders them.
          select.pendingLanding.current = { kind: "typed", key: event.key };
          select.setOpen(true);
          return;
        }
        const items = options();
        const at = items.findIndex((item) => item.id === select.activeId);
        const found = select.typeahead(items, at, event.key);
        if (found != null) {
          // Prevented only when something was found, the way `menu.js` does it:
          // a letter that matches nothing here is a letter the browser's own
          // find-as-you-type may still want.
          event.preventDefault();
          put(found);
        }
      })}
      ref={composeRefs(rest.ref, (element) => {
        select.triggerRef.current = element;
      })}
      role="combobox"
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * What the trigger shows for the current value.
 *
 * The content of a `role="combobox"` element is its *value*, not its name —
 * which is why `Select.Label` exists and why this part may be plain text with
 * no ARIA of its own.
 *
 * Four rules, in order, and the third is the one worth knowing about:
 *
 *   1. `children`, when the caller passed any. A caller who holds the option
 *      list as data already knows what `value` is called and this is how they
 *      say so.
 *   2. The label of the option with that value, learned from the options
 *      themselves the first time the list was rendered and remembered after it
 *      closes.
 *   3. The value itself, when it has never been seen as an option — a select
 *      whose `defaultValue` came from a saved form and whose list has not been
 *      opened yet. A reader hears "GB" rather than "United Kingdom", which is
 *      wrong and true; showing the placeholder there would be wrong and
 *      confident, telling a reader that nothing is chosen when something is.
 *   4. The placeholder, only when nothing is chosen at all.
 *
 * Case 3 is a real edge and the way out of it is case 1.
 */
export component SelectValue(children?: React.Node, placeholder?: React.Node, ...rest: Rest) {
  const select = useSelect("Select.Value");
  const chosen = select.value;

  if (children != null) {
    return <span {...rest}>{children}</span>;
  }
  if (chosen == null) {
    return <span {...rest}>{placeholder}</span>;
  }
  return <span {...rest}>{select.labels[chosen] ?? chosen}</span>;
}

/**
 * The list of options, in the document only while it is open.
 *
 * `div`s rather than `combobox.js`'s `ul`/`li`, because a listbox with groups
 * cannot be a list without putting a second `list` role between a group and the
 * options it owns. The module header has the ARIA rule this follows from.
 *
 * The effect below keeps the one invariant this pattern rests on:
 * `aria-activedescendant` never names an option that is not in the document.
 */
export component SelectList(
  children: renders* (SelectOption | SelectGroup | SelectSeparator),
  align?: Align = "start",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 0,
  side?: LogicalSide = "bottom",
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const select = useSelect("Select.List");
  const { activeId, listRef, pendingLanding, setActiveId, triggerRef, typeahead, value } = select;
  const close = useStableCallback(() => {
    select.setOpen(false);
    select.setActiveId(null);
  });

  // The popup a select opens is the one case where the trigger's *width* is
  // part of the design rather than a detail: a list narrower than the button it
  // came out of reads as a different control. `--uf-anchor-trigger-width` is
  // written on this element for a stylesheet to use, which is why the
  // measurement is here and not in the caller.
  const anchored = useAnchor({
    align,
    alignOffset,
    anchorRef: triggerRef,
    avoidCollisions,
    collisionPadding,
    open: select.open,
    overlayRef: listRef,
    side,
    sideOffset,
  });

  // No dependency list, for the reason `combobox.js` gives: what this reads is
  // the *rendered* options, and a caller may render different ones on any
  // render — a change to `children` that no dependency list can describe. Every
  // write is guarded by a comparison, so it settles after one extra pass rather
  // than looping.
  useEffect(() => {
    const list = listRef.current;
    if (list == null) {
      return;
    }
    const items = itemsOf(list, OPTION_SELECTOR, LISTBOX_SELECTOR);

    const wanted = pendingLanding.current;
    if (wanted != null) {
      pendingLanding.current = null;
      // An `if` rather than a `match` on `wanted.kind`, because matching on a
      // property does not refine the object that property came from: inside
      // `match (wanted.kind)` both arms still see the whole union, and `uf
      // check` says so twice.
      const landing =
        wanted.kind === "typed"
          ? typeahead(items, -1, wanted.key)
          : // The selected option when the key that opened the list meant
            // "start from where I am", and when there is a selection to start
            // from; the end that key named otherwise.
            ((wanted.preferSelected
              ? items.find((item) => item.getAttribute("data-value") === value)
              : null) ?? moveTo(items, -1, wanted.end, false));
      if (landing != null) {
        setActiveId(landing.id);
        (landing as $FlowFixMe).scrollIntoView?.({ block: "nearest" });
      }
      return;
    }

    if (activeId != null && !items.some((item) => item.id === activeId)) {
      // The option the cursor named has left the list. Clearing it is what
      // keeps `aria-activedescendant` pointing only at ids that exist.
      setActiveId(null);
    }
  });

  // Keyed on `select.open`, which is load-bearing: this component is mounted
  // the whole time and only *renders* while the list is open, so keyed on the
  // stable callbacks alone the effect would run once, on the commit where
  // `listRef.current` was still null, and never attach the listener at all.
  useEffect(() => {
    const list = listRef.current;
    if (list == null) {
      return;
    }
    const document = list.ownerDocument;
    const onOutsidePress = (event: Event) => {
      const target: $FlowFixMe = event.target;
      if (target == null || list.contains(target)) {
        return;
      }
      // The trigger is not "outside": closing here and letting its own click
      // reopen the list makes a press on the trigger a no-op that flickers.
      const trigger = triggerRef.current;
      if (trigger != null && trigger.contains(target)) {
        return;
      }
      close();
    };
    document.addEventListener("pointerdown", onOutsidePress, true);
    return () => document.removeEventListener("pointerdown", onOutsidePress, true);
  }, [select.open, close, listRef, triggerRef]);

  if (!select.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["ref"]);

  return (
    <div
      {...passed}
      aria-labelledby={select.labelled ? `${select.base}-label` : undefined}
      data-align={anchored.align}
      data-side={anchored.side}
      id={`${select.base}-list`}
      ref={composeRefs(rest.ref, (element) => {
        listRef.current = element;
      })}
      role="listbox"
    >
      {children}
    </div>
  );
}

/**
 * One option.
 *
 * Never focusable, and that is the invariant the whole pattern rests on: focus
 * belongs to the trigger, and an option that could take it would leave the
 * reader's keystrokes arriving somewhere with no key handler.
 *
 * `data-value` is how the trigger reads back what the cursor is on, because it
 * finds the option in the document rather than in a registry that could
 * disagree with the page — the reason `internal/roving-focus.js` gives.
 */
export component SelectOption(
  value: string,
  children: React.Node,
  label?: string,
  disabled?: boolean = false,
  ...rest: Rest
) {
  const select = useSelect("Select.Option");
  const id = useId();
  const active = select.activeId === id;
  const selected = select.value === value;
  const passed = withoutComposed(rest, ["onClick", "onPointerDown", "onPointerMove", "ref"]);
  const register = select.registerLabel;
  const element = useRef<HTMLElement | null>(null);

  // What `Select.Value` will show once this option has been unmounted with the
  // list. Read from the DOM rather than from `children`, because `children` is
  // a `React.Node` — an icon beside a word, a fragment, a caller's own
  // component — and the only thing that reliably knows what it came out as is
  // the element it came out in. `label` overrides it for the case where the
  // rendered content is not what the trigger should say.
  useEffect(() => {
    register(value, label ?? textOf(element.current));
  }, [register, value, label]);

  return (
    <div
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      aria-selected={selected ? "true" : "false"}
      // For styling the cursor. `data-` rather than a class because this
      // package ships no styles and the caller owns the class list.
      data-active={active ? "true" : undefined}
      data-value={value}
      id={id}
      onClick={composeHandlers(rest.onClick, () => {
        if (!disabled) {
          select.choose(value);
        }
      })}
      // A press must not take focus off the trigger. Without this the trigger
      // blurs on `pointerdown`, and every key the reader presses next arrives
      // at the document instead of at this widget.
      onPointerDown={composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
        event.preventDefault();
      })}
      // The pointer moves the cursor so the keyboard and the mouse agree about
      // which option `Enter` would take.
      onPointerMove={composeHandlers(rest.onPointerMove, () => {
        if (!disabled && !active) {
          select.setActiveId(id);
        }
      })}
      ref={composeRefs(rest.ref, (node) => {
        element.current = node;
      })}
      role="option"
    >
      {children}
    </div>
  );
}

/**
 * A named group of options.
 *
 * The name reaches the group through `aria-labelledby`, and only while a
 * `Select.GroupLabel` is rendered — the same rule, and the same reason, as
 * `Menu.Group`. The arrow keys pass over the label without stopping on it,
 * because they only ever look for `role="option"`.
 */
export component SelectGroup(children: React.Node, ...rest: Rest) {
  const base = useId();
  const [labelled, setLabelled] = useState(false);

  const group = useMemo(() => ({ labelId: `${base}-label`, registerLabel: setLabelled }), [base]);

  return (
    <SelectGroupContext.Provider value={group}>
      <div {...rest} aria-labelledby={labelled ? group.labelId : undefined} role="group">
        {children}
      </div>
    </SelectGroupContext.Provider>
  );
}

/**
 * The heading of a `Select.Group`.
 *
 * `role="presentation"` because the group already carries the name: left as
 * ordinary content a reader would hear the heading once as the group's name and
 * again as a stray line of text among the options.
 */
export component SelectGroupLabel(children: React.Node, ...rest: Rest) {
  const group = useContext(SelectGroupContext);
  const register = group?.registerLabel;

  useEffect(() => {
    if (register == null) {
      return;
    }
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <div {...rest} id={group?.labelId} role="presentation">
      {children}
    </div>
  );
}

/**
 * A rule between groups of options.
 *
 * `aria-hidden`, and this is the one part that differs from `Menu.Separator`. A
 * `separator` is a legal child of a `menu` and is announced there; a `listbox`
 * may only own `option` and `group`, so a separator inside one is a child ARIA
 * does not allow, and what a reader is told about an invalid listbox is up to
 * the software rather than the specification. The rule between two groups is
 * decoration, so it says so and stays out of the tree.
 */
export component SelectSeparator(...rest: Rest) {
  return <div {...rest} aria-hidden="true" />;
}

/** What an option came out as, for the trigger to show later. */
function textOf(element: HTMLElement | null): string {
  return (element?.textContent ?? "").replace(/\s+/g, " ").trim();
}
