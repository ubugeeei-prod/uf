// @flow
//
// A combobox: a text field with a list of options attached to it.
//
// It is the one widget in this package where focus does *not* move onto the
// items, and everything else about it follows from that. Focus has to stay in
// the text field — the reader is still typing — so the list is navigated with
// `aria-activedescendant`, a second, "virtual" cursor that names which option is
// current while the real one stays put. Getting that wrong is the classic
// broken autocomplete: the arrow keys move a highlight the sighted reader can
// see and the screen reader says nothing, because nothing it watches changed.
//
// The keyboard map, and what each key is protecting:
//
//   * `ArrowDown` / `ArrowUp` open the list and move the active option, wrapping
//     at the ends.
//   * `Alt+ArrowDown` opens the list *without* moving, and `Alt+ArrowUp` closes
//     it. This is how a reader looks at the options without committing to one.
//   * `Enter` takes the active option. With no active option it does nothing —
//     which is deliberate, because that is what lets a combobox inside a form
//     still submit it.
//   * `Escape` closes the list; pressed again, with the list already closed, it
//     clears the field. It is also stopped from travelling any further, so a
//     combobox inside a dialog does not close the dialog on the way past.
//   * `Tab` closes the list and moves on *without* selecting. A list that
//     commits whatever happened to be highlighted turns a keystroke meant to
//     leave the field into an edit.
//   * `Home` and `End` are deliberately left alone. They belong to the text
//     cursor, and a combobox that steals them to jump to the first and last
//     option has made its own text field harder to edit than a plain `<input>`.
//
// # The announcement
//
// A screen reader reader who types "ma" needs to be told that four options
// matched, and nothing about the list appearing says so: the options are not in
// the reading order, and `aria-activedescendant` only speaks when one becomes
// current. `Combobox.Status` is a polite live region carrying that count. It is
// rendered whether the list is open or not, on purpose — a live region inserted
// into the document at the same moment as its content is usually not announced
// at all, because the region has to be there to be watched before the thing it
// is watching changes.
//
// # Filtering belongs to the caller
//
// This component never filters. The options are whatever the caller rendered,
// and matching against `Combobox.Root`'s `inputValue` is application logic —
// fuzzy or prefix, accent-folding or not, local or from a server. What the
// component owns is everything that has to stay true *while* the list changes:
// the active option is cleared when the option it named is filtered away, the
// count is remeasured, and `aria-activedescendant` never names an id that has
// left the document.

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

import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import { itemsOf, moveTo } from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";

const OPTION_SELECTOR = '[role="option"]';
const LISTBOX_SELECTOR = '[role="listbox"]';

type ComboboxState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** The chosen option's value, or null when nothing is chosen. */
  readonly value: string | null,
  /** The text in the field, which is not the value until something is chosen. */
  readonly text: string,
  readonly setText: (text: string) => void,
  /** Take an option: sets the value, puts its label in the field, closes. */
  readonly select: (value: string, label: string) => void,
  /** Empty the field and the selection, which is what a second Escape does. */
  readonly clear: () => void,
  /** The id of the option `aria-activedescendant` names, if any. */
  readonly activeId: string | null,
  readonly setActiveId: (id: string | null) => void,
  /**
   * Which end to activate once the list is in the document.
   *
   * `ArrowDown` on a closed combobox opens it *and* lands on the first option,
   * and the list does not exist to be measured until the next commit. A ref
   * rather than state because nothing renders it.
   */
  readonly pendingActive: { current: "first" | "last" | null },
  readonly inputRef: { current: HTMLElement | null },
  readonly listRef: { current: HTMLElement | null },
  /** How many options are in the list, for the live region. */
  readonly count: number,
  readonly setCount: (count: number) => void,
  readonly labelled: boolean,
  readonly registerLabel: (present: boolean) => void,
|};

const ComboboxContext: React.Context<ComboboxState | null> = createContext(null);

hook useCombobox(part: string): ComboboxState {
  const state = useContext(ComboboxContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Combobox.Root`);
  }
  return state;
}

/**
 * The combobox.
 *
 * Three separate things a caller may own, because applications own different
 * ones: `value` is what has been chosen, `inputValue` is what is typed, and
 * `open` is whether the list is showing. A search box owns the text and nothing
 * else; a form field owns the value; a page with a "browse all" button owns
 * `open`. Tying them together would make two of those three impossible.
 */
export component ComboboxRoot(
  children: React.Node,
  value?: string | null,
  defaultValue?: string | null = null,
  onValueChange?: (value: string | null) => void,
  inputValue?: string,
  defaultInputValue?: string = "",
  onInputValueChange?: (text: string) => void,
  open?: boolean,
  defaultOpen?: boolean = false,
  onOpenChange?: (open: boolean) => void,
  ...rest: { readonly [string]: mixed }
) {
  const base = useId();
  const [chosen, setChosen] = useControlled(value, defaultValue, onValueChange);
  const [text, setText] = useControlled(inputValue, defaultInputValue, onInputValueChange);
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [count, setCount] = useState(0);
  const [labelled, setLabelled] = useState(false);
  const pendingActive = useRef<"first" | "last" | null>(null);
  const inputRef = useRef<HTMLElement | null>(null);
  const listRef = useRef<HTMLElement | null>(null);

  // Stable, so the parts below can hold on to them without re-subscribing every
  // time the caller re-renders with a fresh `onValueChange`.
  const select = useStableCallback((next: string, label: string) => {
    setChosen(next);
    setText(label);
    setOpen(false);
    setActiveId(null);
    // Focus never left the field for a keyboard selection; it did for a click
    // on an option, and it has to come back or the next keystroke goes nowhere.
    inputRef.current?.focus();
  });

  const clear = useStableCallback(() => {
    setChosen(null);
    setText("");
    setActiveId(null);
  });

  const state = useMemo(
    () => ({
      base,
      open: isOpen,
      setOpen,
      value: chosen,
      text,
      setText,
      select,
      clear,
      activeId,
      setActiveId,
      pendingActive,
      inputRef,
      listRef,
      count,
      setCount,
      labelled,
      registerLabel: setLabelled,
    }),
    [base, isOpen, setOpen, chosen, text, setText, select, clear, activeId, count, labelled],
  );

  return (
    <ComboboxContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </ComboboxContext.Provider>
  );
}

/**
 * The field's label.
 *
 * A real `<label for>`, so clicking it focuses the field and so the name comes
 * from the same place for the field and for the list. It registers itself
 * because the list names it, and naming a label that is not rendered is worse
 * than leaving the list unnamed.
 */
export component ComboboxLabel(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const combobox = useCombobox("Combobox.Label");
  const register = combobox.registerLabel;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <label {...rest} htmlFor={`${combobox.base}-input`} id={`${combobox.base}-label`}>
      {children}
    </label>
  );
}

/** The text field, and every key the pattern defines. */
export component ComboboxInput(...rest: { readonly [string]: mixed }) {
  const combobox = useCombobox("Combobox.Input");
  const passed = withoutComposed(rest, ["onChange", "onKeyDown", "ref"]);

  /** The options in the document right now, in document order. */
  const options = (): Array<HTMLElement> => {
    const list = combobox.listRef.current;
    return list == null ? [] : itemsOf(list, OPTION_SELECTOR, LISTBOX_SELECTOR);
  };

  const move = (movement: "previous" | "next") => {
    const items = options();
    if (items.length === 0) {
      // The list is not in the document yet, so leave an instruction for the
      // commit that puts it there.
      combobox.pendingActive.current = movement === "next" ? "first" : "last";
      return;
    }
    const at = items.findIndex((item) => item.id === combobox.activeId);
    const next = moveTo(items, at, movement, true);
    if (next == null) {
      return;
    }
    combobox.setActiveId(next.id);
    // `nearest`, so a list that is already showing the option does not jump.
    (next as $FlowFixMe).scrollIntoView?.({ block: "nearest" });
  };

  const take = (element: HTMLElement) => {
    combobox.select(element.getAttribute("data-value") ?? "", labelOf(element));
  };

  return (
    <input
      {...passed}
      // Only while the list is in the document. `aria-activedescendant` naming
      // an option that has been filtered away, or `aria-controls` naming a
      // listbox that is not rendered, both make a screen reader announce
      // nothing rather than announce something slightly wrong.
      aria-activedescendant={combobox.open ? (combobox.activeId ?? undefined) : undefined}
      // "list": the field's own text is never rewritten by the component, so
      // this is not `both` (inline completion) and not `none`.
      aria-autocomplete="list"
      aria-controls={combobox.open ? `${combobox.base}-list` : undefined}
      aria-expanded={combobox.open ? "true" : "false"}
      // The browser's own dropdown would sit on top of this one.
      autoComplete="off"
      id={`${combobox.base}-input`}
      onChange={composeHandlers(rest.onChange, (event: $FlowFixMe) => {
        combobox.setText(event.target.value);
        combobox.setOpen(true);
        // Typing invalidates the highlight: the option that was current may not
        // even be in the filtered list any more, and carrying it over means
        // Enter takes something the reader can no longer see.
        combobox.setActiveId(null);
      })}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          if (event.altKey) {
            // Look without moving, and close without choosing.
            combobox.setOpen(event.key === "ArrowDown");
            return;
          }
          combobox.setOpen(true);
          move(event.key === "ArrowDown" ? "next" : "previous");
          return;
        }

        if (event.key === "Enter") {
          const chosen = options().find((item) => item.id === combobox.activeId);
          if (!combobox.open || chosen == null) {
            // Nothing is highlighted, so this keystroke is the form's.
            return;
          }
          event.preventDefault();
          take(chosen);
          return;
        }

        if (event.key === "Escape") {
          event.preventDefault();
          // A dialog around this combobox must not also close: one Escape is
          // one dismissal, and the innermost thing wins.
          event.stopPropagation();
          if (combobox.open) {
            combobox.setOpen(false);
            combobox.setActiveId(null);
          } else {
            combobox.clear();
          }
          return;
        }

        if (event.key === "Tab" && combobox.open) {
          // Not prevented, and nothing is taken: Tab is how a reader leaves a
          // field, not how they commit to a highlight they were only passing.
          combobox.setOpen(false);
          combobox.setActiveId(null);
        }
      })}
      ref={composeRefs(rest.ref, (element) => {
        combobox.inputRef.current = element;
      })}
      role="combobox"
      type="text"
      value={combobox.text}
    />
  );
}

/**
 * The list of options, in the document only while it is open.
 *
 * It also keeps the two things that have to stay true as the caller filters:
 * the count the live region announces, and the invariant that
 * `aria-activedescendant` never names an option that has left the list.
 */
export component ComboboxList(
  children: renders* ComboboxOption,
  ...rest: { readonly [string]: mixed }
) {
  const combobox = useCombobox("Combobox.List");
  const { activeId, count, listRef, inputRef, pendingActive, setActiveId, setCount } = combobox;
  const close = useStableCallback(() => {
    combobox.setOpen(false);
    combobox.setActiveId(null);
  });

  // No dependency list on purpose: what this reads is the *rendered* options,
  // and they change whenever the caller re-filters — which is a change to
  // `children` that no dependency list can describe. Every write below is
  // guarded by a comparison, so the effect settles after one extra pass rather
  // than looping.
  useEffect(() => {
    const list = listRef.current;
    if (list == null) {
      // Closed. The live region must not keep announcing options that are no
      // longer in the document.
      if (count !== 0) {
        setCount(0);
      }
      return;
    }
    const items = itemsOf(list, OPTION_SELECTOR, LISTBOX_SELECTOR);
    if (items.length !== count) {
      setCount(items.length);
    }

    const wanted = pendingActive.current;
    if (wanted != null) {
      pendingActive.current = null;
      setActiveId(moveTo(items, -1, wanted, false)?.id ?? null);
      return;
    }
    if (activeId != null && !items.some((item) => item.id === activeId)) {
      // The active option was filtered away. Clearing it is what keeps
      // `aria-activedescendant` pointing only at ids that exist.
      setActiveId(null);
    }
  });

  // Keyed on `combobox.open`, and that is load-bearing. This component is
  // mounted the whole time and only *renders* while the list is open, so keyed
  // on the stable callbacks alone the effect ran once — on the first commit,
  // when `listRef.current` was still null — and never again. The listener was
  // never attached, and a press outside the combobox closed nothing.
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
      // The field is not "outside": pressing it is how a reader gets back to
      // typing, and closing on it would fight the input's own handlers.
      const input = inputRef.current;
      if (input != null && input.contains(target)) {
        return;
      }
      close();
    };
    document.addEventListener("pointerdown", onOutsidePress, true);
    return () => document.removeEventListener("pointerdown", onOutsidePress, true);
  }, [combobox.open, close, listRef, inputRef]);

  if (!combobox.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["ref"]);

  return (
    <ul
      {...passed}
      aria-labelledby={combobox.labelled ? `${combobox.base}-label` : undefined}
      id={`${combobox.base}-list`}
      ref={composeRefs(rest.ref, (element) => {
        listRef.current = element;
      })}
      role="listbox"
    >
      {children}
    </ul>
  );
}

/**
 * One option.
 *
 * Never focusable: focus belongs to the text field, and an option that can take
 * it would break the one invariant this pattern rests on. `data-value` and
 * `data-label` are how the field reads back what was chosen, because the field
 * finds the active option in the document rather than in a registry that could
 * disagree with it.
 */
export component ComboboxOption(
  value: string,
  children: React.Node,
  label?: string,
  disabled?: boolean = false,
  ...rest: { readonly [string]: mixed }
) {
  const combobox = useCombobox("Combobox.Option");
  const id = useId();
  const active = combobox.activeId === id;
  const passed = withoutComposed(rest, ["onClick", "onPointerDown", "onPointerMove"]);

  return (
    <li
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      aria-selected={combobox.value === value ? "true" : "false"}
      // For styling the highlight. It is `data-` rather than a class because
      // this package ships no styles and the caller owns the class list.
      data-active={active ? "true" : undefined}
      data-label={label}
      data-value={value}
      id={id}
      onClick={composeHandlers(rest.onClick, (event: $FlowFixMe) => {
        if (disabled) {
          return;
        }
        combobox.select(value, label ?? textOf(event.currentTarget));
      })}
      // A press must not take focus off the field. Without this the field blurs
      // on `mousedown`, the list closes, and the `click` that follows lands on
      // nothing — which is why so many autocompletes cannot be clicked at all.
      onPointerDown={composeHandlers(rest.onPointerDown, (event: $FlowFixMe) => {
        event.preventDefault();
      })}
      // The pointer moves the highlight so the keyboard and the mouse agree on
      // which option `Enter` would take.
      onPointerMove={composeHandlers(rest.onPointerMove, () => {
        if (!disabled && !active) {
          combobox.setActiveId(id);
        }
      })}
      role="option"
    >
      {children}
    </li>
  );
}

/**
 * What to show when the caller filtered everything away.
 *
 * Rendered beside the list rather than inside it, because a listbox may only
 * contain options: an "no matches" row inside one is announced as an option a
 * reader can choose, and choosing it does nothing.
 */
export component ComboboxEmpty(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const combobox = useCombobox("Combobox.Empty");
  if (!combobox.open || combobox.count > 0) {
    return null;
  }
  return <div {...rest}>{children}</div>;
}

/**
 * The live region that tells a screen reader how many options matched.
 *
 * Always in the document, even when the list is closed. A live region added to
 * the page in the same commit as the text it holds is usually not announced,
 * because the technology watching it had nothing to watch until it was already
 * too late; leaving it mounted and empty is what makes the *next* change speak.
 *
 * `children` overrides the wording — the default is English and a real
 * application has a translation table.
 */
export component ComboboxStatus(children?: React.Node, ...rest: { readonly [string]: mixed }) {
  const combobox = useCombobox("Combobox.Status");
  const message = children ?? defaultAnnouncement(combobox.open, combobox.count);

  return (
    <div {...rest} aria-atomic="true" aria-live="polite" role="status">
      {message}
    </div>
  );
}

/** The wording `Combobox.Status` uses when the caller supplies none. */
function defaultAnnouncement(open: boolean, count: number): string {
  if (!open) {
    return "";
  }
  if (count === 0) {
    return "No results available.";
  }
  return count === 1 ? "1 result available." : `${count} results available.`;
}

/** What a reader hears for an option: its explicit label, or its own text. */
function labelOf(element: HTMLElement): string {
  return element.getAttribute("data-label") ?? textOf(element);
}

function textOf(element: HTMLElement): string {
  return (element.textContent ?? "").replace(/\s+/g, " ").trim();
}
