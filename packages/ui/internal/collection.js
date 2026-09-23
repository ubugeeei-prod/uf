// @flow
"use client";
//
// The one collection behind `ListBox`, `GridList`, `Tree` and `TagGroup`.
//
// Focus stays on the collection element and `aria-activedescendant` names the
// active row, so a virtualised list of 10,000 rows needs only the rows in view
// plus the active one in the document. What a gesture does to the selection is
// `selection.js`'s answer; this file maps keys and pointers onto those answers
// and keeps the anchor a Shift range grows from.
//
// The keyboard map (WAI-ARIA APG listbox, grid and tree patterns, with React
// Aria's selection manager for the parts the APG leaves open):
//
//   ArrowUp/ArrowDown     previous/next row (TagGroup: also Left/Right,
//                         mirrored in right-to-left text)
//   Home/End              first/last row; End also asks for more rows
//   PageUp/PageDown       a viewport's worth of rows
//   Shift + any of those  extend the selection from the anchor
//   Ctrl/Cmd + any        move without selecting (`selectionBehavior="replace"`)
//   Space                 select (toggle or replace, per `selectionBehavior`)
//   Ctrl/Cmd+Space        toggle one row — or lift it, when `onReorder` is set
//   Enter                 `onAction`, or select when there is none
//   Ctrl/Cmd+A            select every enabled row
//   Escape                clear the selection (`escapeKeyBehavior`)
//   Tree: Right/Left      expand, enter; collapse, go to parent (mirrored)
//   Tree: *               expand every sibling of the active row
//   printable characters  locale-collated typeahead
//
// The Escape key is only claimed when it cleared something, so a collection
// inside a popover still lets the popover close on the Escape that follows.

import * as React from "@uniflowed/react";
import { useId, useRef, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import { useDragAndDrop } from "../drag-drop.js";
import { useControlled } from "./controlled-state.js";
import { composeHandlers, composeRefs, withProps } from "./merge-props.js";
import type { PartEvent, RenderProp } from "./merge-props.js";
import { directionOf } from "./roving-focus.js";
import { clearAll, extendTo, orderedKeys, replaceWith, selectAll, toggleKey } from "./selection.js";
import type { SelectionBehavior, SelectionMode, SelectionPolicy } from "./selection.js";
import { startsWithLocale, useLocale } from "../i18n-provider.js";
import { visuallyHiddenStyle } from "./visually-hidden-style.js";
import { formatCount, useMessages } from "./messages.js";

export type CollectionItem = {
  readonly key: string,
  readonly textValue: string,
  readonly disabled?: boolean,
  readonly children?: $ReadOnlyArray<CollectionItem>,
};
export type CollectionItemState = {
  readonly selected: boolean,
  readonly active: boolean,
  readonly disabled: boolean,
  readonly level: number,
};
type Entry = {
  item: CollectionItem,
  level: number,
  parent: string | null,
  position: number,
  count: number,
};
type Kind = "listbox" | "grid" | "tree" | "tags";

/**
 * What a row's click handler reads.
 *
 * `detail` is the click count: `0` for a click no pointer made — a screen
 * reader's activation, or `element.click()` — which React Aria calls a virtual
 * click. `nativeEvent` is read for `pointerType`, which a browser puts on the
 * `PointerEvent` a click is and happy-dom and older engines leave off.
 */
type RowClick = {
  readonly detail: number,
  readonly ctrlKey: boolean,
  readonly metaKey: boolean,
  readonly shiftKey: boolean,
  readonly nativeEvent: mixed,
  readonly stopPropagation: () => mixed,
  ...
};

type Scroll = { readonly currentTarget: mixed, readonly defaultPrevented: boolean, ... };

/** What the collection's key handler reads: `PartEvent` plus the element the key came from. */
type KeyEvent = { ...PartEvent, readonly target: mixed, ... };

function entries(
  items: $ReadOnlyArray<CollectionItem>,
  expanded: $ReadOnlySet<string>,
  tree: boolean,
): Array<Entry> {
  const result = [];
  const seen = new Set<string>();
  const visit = (
    siblings: $ReadOnlyArray<CollectionItem>,
    level: number,
    parent: string | null,
  ): void => {
    siblings.forEach((item, index) => {
      if (seen.has(item.key)) throw new Error(`Duplicate collection key: ${item.key}`);
      seen.add(item.key);
      result.push({ item, level, parent, position: index + 1, count: siblings.length });
      const children = item.children;
      if (tree && children != null && expanded.has(item.key)) visit(children, level + 1, item.key);
    });
  };
  visit(items, 1, null);
  return result;
}

/**
 * A touch or a virtual click toggles even under `selectionBehavior="replace"`.
 *
 * There is no Ctrl key on a phone and no modifier on a screen reader's
 * activation, so a replace-only rule would leave both able to choose one row
 * and never two. React Aria draws the same line.
 */
function togglesByPointer(event: RowClick): boolean {
  if (event.detail === 0) return true;
  const native = event.nativeEvent;
  return typeof native === "object" && native != null && native.pointerType === "touch";
}

/** Data owns order and identity. Only visible tree descendants and virtual rows enter the DOM. */
export component CollectionRoot(kind: Kind, options: CollectionProps) {
  const {
    items,
    children,
    selectionMode: requestedSelectionMode = "single",
    selectionBehavior = "toggle",
    disallowEmptySelection = false,
    escapeKeyBehavior = "clearSelection",
    selectedKeys,
    defaultSelectedKeys = [],
    onSelectionChange,
    onAction,
    disabledKeys = [],
    expandedKeys,
    defaultExpandedKeys = [],
    onExpandedChange,
    onRemove,
    onReorder,
    loading = false,
    onLoadMore,
    virtualized = false,
    height = 300,
    rowHeight = 30,
    render,
    ...rest
  } = options;
  const mode: SelectionMode = kind === "tags" ? "none" : requestedSelectionMode;
  const policy: SelectionPolicy = {
    mode,
    behavior: selectionBehavior,
    disallowEmpty: disallowEmptySelection,
  };
  if (height <= 0 || rowHeight <= 0 || !Number.isFinite(height) || !Number.isFinite(rowHeight))
    throw new RangeError("Collection dimensions must be positive finite numbers");
  const { locale } = useLocale();
  const messages = useMessages(locale);
  const id = useId();
  const root = useRef<HTMLElement | null>(null);
  // Where a Shift range starts (`anchor`) and where the last one ended (`lead`).
  // Written only by event handlers, never read during render.
  const anchor = useRef<string | null>(null);
  const lead = useRef<string | null>(null);
  const buffer = useRef({ text: "", time: 0 });
  const [selected, setSelected] = useControlled(
    selectedKeys,
    defaultSelectedKeys,
    onSelectionChange,
  );
  const [expanded, setExpanded] = useControlled(
    expandedKeys,
    defaultExpandedKeys,
    onExpandedChange,
  );
  const [active, setActive] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [announcement, announce] = useState("");
  // Sets, because every row asks "am I selected, am I disabled" on every render
  // and a 10,000-row collection with everything selected made that 10⁸
  // comparisons as arrays.
  // The React Compiler keeps each of these until its input changes.
  const selectedSet = new Set(selected);
  const disabledSet = new Set(disabledKeys);
  const expandedSet = new Set(expanded);
  const rows = entries(items, expandedSet, kind === "tree");
  const disabled = (item: CollectionItem) => item.disabled === true || disabledSet.has(item.key);
  const enabled = rows.filter((row) => !disabled(row.item));
  const order = enabled.map((row) => row.item.key);
  const drag = useDragAndDrop({
    disabled: onReorder == null,
    onDrop: ({ keys, target }) => {
      const moving = keys[0];
      const byKey = (key: string) => items.find((item) => item.key === key);
      const from = byKey(moving);
      const to = byKey(target);
      if (from == null || to == null || moving === target || disabled(from) || disabled(to)) return;
      const ordered = items.filter((item) => item.key !== moving).map((item) => item.key);
      ordered.splice(ordered.indexOf(target), 0, moving);
      onReorder?.(ordered);
    },
  });
  const focused =
    enabled.find((row) => row.item.key === active) ??
    enabled.find((row) => selectedSet.has(row.item.key)) ??
    enabled[0];
  const activeKey = focused?.item.key;

  /** Report a selection, unless the gesture changed nothing. */
  const commit = (next: $ReadOnlySet<string>) => {
    if (next === selectedSet) return;
    const keys = orderedKeys(next, order);
    setSelected(keys);
    announce(messages.selectedCount(keys.length, formatCount(keys.length, locale)));
  };
  /** A plain or Ctrl/Cmd gesture on one row: toggle or replace, and move the anchor. */
  const selectOne = (key: string, toggle: boolean) => {
    if (mode === "none") return;
    commit(
      toggle || selectionBehavior === "toggle"
        ? toggleKey(policy, selectedSet, key)
        : replaceWith(policy, selectedSet, key),
    );
    anchor.current = key;
    lead.current = key;
  };
  /** A Shift gesture: grow from the anchor, which is the active row if nothing set one. */
  const extend = (key: string, from: string | void) => {
    if (anchor.current == null || !order.includes(anchor.current)) anchor.current = from ?? key;
    commit(extendTo(policy, selectedSet, order, anchor.current, lead.current, key));
    lead.current = key;
  };
  const scrollTo = (row: Entry) => {
    const container = root.current;
    if (virtualized && container != null) {
      const top = rows.indexOf(row) * rowHeight;
      if (top < container.scrollTop || top + rowHeight > container.scrollTop + height) {
        container.scrollTop = top;
        setScrollTop(top);
      }
    }
  };
  /** Move the active row, and select the way the modifiers ask. */
  const land = (
    row: Entry,
    event: {
      readonly shiftKey: boolean,
      readonly ctrlKey: boolean,
      readonly metaKey: boolean,
      ...
    },
  ) => {
    const previous = activeKey;
    setActive(row.item.key);
    scrollTo(row);
    if (mode === "none") return;
    if (event.shiftKey && mode === "multiple") extend(row.item.key, previous);
    else if (selectionBehavior === "replace" && !event.ctrlKey && !event.metaKey)
      selectOne(row.item.key, false);
  };
  /** Rows per PageUp/PageDown: the viewport over one row, or ten when nothing is laid out. */
  const pageSize = (): number => {
    if (virtualized) return Math.max(1, Math.floor(height / rowHeight));
    const container = root.current;
    const row = container?.querySelector("[data-key]");
    if (container != null && row instanceof HTMLElement && row.offsetHeight > 0)
      return Math.max(1, Math.floor(container.clientHeight / row.offsetHeight));
    return 10;
  };
  const remove = (key: string) => {
    const at = enabled.findIndex((row) => row.item.key === key);
    if (at < 0 || onRemove == null) return;
    onRemove(key);
    setActive(enabled[at + 1]?.item.key ?? enabled[at - 1]?.item.key ?? null);
    announce(messages.removed(enabled[at].item.textValue));
  };
  const keydown = useStableCallback((event: KeyEvent) => {
    const { target, currentTarget } = event;
    if (!(currentTarget instanceof HTMLElement)) return;
    if (
      target !== currentTarget &&
      target instanceof Element &&
      target.closest("button,input,textarea,select") != null
    )
      return;
    const modifier = event.ctrlKey || event.metaKey;
    if (drag.dragging && event.key === "Escape") {
      event.preventDefault();
      drag.cancel();
      return;
    }
    if (drag.dragging && event.key === "Enter" && activeKey != null) {
      event.preventDefault();
      drag.drop(activeKey, undefined, focused?.item.textValue);
      return;
    }
    if (onReorder != null && event.key === " " && modifier && focused != null) {
      event.preventDefault();
      drag.start(focused.item.key, focused.item.textValue);
      return;
    }
    const rtl = directionOf(currentTarget) === "rtl";
    const inline = kind === "tags";
    const forward =
      event.key === "ArrowDown" || (inline && event.key === (rtl ? "ArrowLeft" : "ArrowRight"));
    const backward =
      event.key === "ArrowUp" || (inline && event.key === (rtl ? "ArrowRight" : "ArrowLeft"));
    const at = enabled.findIndex((row) => row.item.key === activeKey);
    let next = null;
    if (forward) next = enabled[Math.min(at + 1, enabled.length - 1)];
    else if (backward) next = enabled[Math.max(at - 1, 0)];
    else if (event.key === "Home") next = enabled[0];
    else if (event.key === "End") {
      next = enabled[enabled.length - 1];
      if (!loading) onLoadMore?.();
    } else if (event.key === "PageDown")
      next = enabled[Math.min(Math.max(at, 0) + pageSize(), enabled.length - 1)];
    else if (event.key === "PageUp") next = enabled[Math.max(at - pageSize(), 0)];
    else if (kind === "tree" && focused != null && event.key === "*") {
      const siblings = rows.filter(
        (row) => row.parent === focused.parent && (row.item.children?.length ?? 0) > 0,
      );
      const missing = siblings.map((row) => row.item.key).filter((key) => !expandedSet.has(key));
      if (missing.length > 0) setExpanded([...expanded, ...missing]);
    } else if (
      kind === "tree" &&
      focused != null &&
      (event.key === "ArrowLeft" || event.key === "ArrowRight")
    ) {
      const open = event.key === (rtl ? "ArrowLeft" : "ArrowRight");
      const key = focused.item.key;
      if (open && (focused.item.children?.length ?? 0) > 0) {
        if (!expandedSet.has(key)) setExpanded([...expanded, key]);
        else next = enabled[at + 1];
      } else if (!open && expandedSet.has(key))
        setExpanded(expanded.filter((each) => each !== key));
      else if (!open) next = enabled.find((row) => row.item.key === focused.parent);
    } else if (modifier && event.key.toLowerCase() === "a" && !event.altKey) {
      if (mode !== "multiple") return;
      commit(selectAll(policy, selectedSet, order));
    } else if (event.key === "Escape") {
      if (escapeKeyBehavior !== "clearSelection") return;
      const cleared = clearAll(policy, selectedSet);
      // Unclaimed when nothing changed, so an enclosing popover still closes.
      if (cleared === selectedSet) return;
      commit(cleared);
    } else if (event.key === "Enter" && activeKey != null && onAction != null) {
      onAction(activeKey);
    } else if ((event.key === " " || event.key === "Enter") && activeKey != null) {
      if (event.shiftKey && mode === "multiple") extend(activeKey, activeKey);
      else selectOne(activeKey, modifier);
    } else if (
      (event.key === "Delete" || event.key === "Backspace") &&
      kind === "tags" &&
      activeKey != null
    )
      remove(activeKey);
    else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
      const time = Date.now();
      const text = time - buffer.current.time > 500 ? event.key : buffer.current.text + event.key;
      buffer.current = { text, time };
      const repeated = Array.from(text).every((char) => char === event.key);
      const query = repeated ? event.key : text;
      const start = repeated || text.length === 1 ? at + 1 : Math.max(at, 0);
      for (let offset = 0; offset < enabled.length; offset += 1) {
        const candidate = enabled[(start + offset) % enabled.length];
        if (startsWithLocale(candidate.item.textValue, query, locale)) {
          next = candidate;
          break;
        }
      }
      // Typeahead moves; it never extends. Under `replace` the selection follows.
      if (next != null) {
        event.preventDefault();
        land(next, { shiftKey: false, ctrlKey: false, metaKey: false });
        return;
      }
    } else return;
    event.preventDefault();
    if (next != null) land(next, event);
  });
  const rowClick = (item: CollectionItem, event: RowClick) => {
    if (disabled(item)) return;
    root.current?.focus();
    setActive(item.key);
    // With nothing to select, a click is the row's action.
    if (mode === "none") {
      onAction?.(item.key);
      return;
    }
    if (event.shiftKey && mode === "multiple") extend(item.key, activeKey);
    else selectOne(item.key, event.ctrlKey || event.metaKey || togglesByPointer(event));
  };
  const start = virtualized ? Math.max(0, Math.floor(scrollTop / rowHeight) - 2) : 0;
  const end = virtualized
    ? Math.min(rows.length, start + Math.ceil(height / rowHeight) + 4)
    : rows.length;
  // aria-activedescendant must always name a mounted row, even after a pointer scroll.
  const indices = new Set(rows.slice(start, end).map((_, index) => start + index));
  const activeIndex = rows.findIndex((row) => row.item.key === activeKey);
  if (activeIndex >= 0) indices.add(activeIndex);
  const rowNodes = Array.from(indices)
    .sort((a, b) => a - b)
    .map((index) => {
      const row = rows[index];
      const { item } = row;
      const chosen = selectedSet.has(item.key);
      const grid = kind === "grid" || kind === "tags";
      const content =
        children?.(item, {
          selected: chosen,
          active: activeKey === item.key,
          disabled: disabled(item),
          level: row.level,
        }) ?? item.textValue;
      const props = {
        ...drag.getDragProps(item.key, item.textValue),
        ...drag.getDropProps(item.key, item.textValue),
        onKeyDown: undefined,
        draggable: onReorder != null && !disabled(item),
        id: `${id}-${index}`,
        role: grid ? "row" : kind === "tree" ? "treeitem" : "option",
        "aria-selected": mode === "none" ? undefined : chosen,
        "aria-disabled": disabled(item) || undefined,
        "aria-expanded":
          kind === "tree" && (item.children?.length ?? 0) > 0
            ? expandedSet.has(item.key)
            : undefined,
        "aria-level": kind === "tree" ? row.level : undefined,
        "aria-posinset": grid ? undefined : kind === "tree" ? row.position : index + 1,
        "aria-setsize": grid ? undefined : kind === "tree" ? row.count : rows.length,
        "aria-rowindex": grid ? index + 1 : undefined,
        "data-key": item.key,
        "data-active": activeKey === item.key || undefined,
        style: virtualized
          ? { position: "absolute", top: index * rowHeight, height: rowHeight, width: "100%" }
          : undefined,
        onClick: (event: RowClick) => rowClick(item, event),
        // Under `replace` a click selects, so the action needs a second gesture.
        onDoubleClick:
          onAction != null && selectionBehavior === "replace" && mode !== "none"
            ? () => {
                if (!disabled(item)) onAction(item.key);
              }
            : undefined,
      };
      return (
        <div key={item.key} {...props}>
          {grid ? (
            <div role="gridcell">
              {content}
              {kind === "tags" && onRemove != null ? (
                <button
                  type="button"
                  tabIndex={-1}
                  disabled={disabled(item)}
                  aria-label={messages.removeItem(item.textValue)}
                  onClick={(event: RowClick) => {
                    event.stopPropagation();
                    root.current?.focus();
                    remove(item.key);
                  }}
                >
                  ×
                </button>
              ) : null}
            </div>
          ) : (
            content
          )}
        </div>
      );
    });
  const setRef = useStableCallback((element: HTMLElement | null) => {
    root.current = element;
  });
  const style = rest.style;
  const props = withProps(rest, {
    ref: composeRefs(rest.ref, setRef),
    role: kind === "tags" ? "grid" : kind,
    tabIndex: 0,
    "aria-activedescendant": activeIndex < 0 ? undefined : `${id}-${activeIndex}`,
    "aria-multiselectable": mode === "multiple" || undefined,
    "aria-busy": loading || undefined,
    "aria-rowcount": kind === "grid" || kind === "tags" ? rows.length : undefined,
    onKeyDown: composeHandlers(rest.onKeyDown, keydown),
    onScroll: composeHandlers(rest.onScroll, (event: Scroll) => {
      const element = event.currentTarget;
      if (!(element instanceof HTMLElement)) return;
      if (virtualized) setScrollTop(element.scrollTop);
      if (!loading && element.scrollTop + element.clientHeight >= element.scrollHeight)
        onLoadMore?.();
    }),
    style: virtualized
      ? {
          ...(typeof style === "object" && style != null ? style : {}),
          height,
          overflow: "auto",
          position: "relative",
        }
      : style,
    children: virtualized ? (
      <div role="presentation" style={{ height: rows.length * rowHeight, position: "relative" }}>
        {rowNodes}
      </div>
    ) : (
      rowNodes
    ),
  });
  return (
    <>
      {render != null ? render(props) : <div {...props} />}
      {/* For a screen reader; a sighted reader sees the selection itself. */}
      <span role="status" aria-live="polite" style={visuallyHiddenStyle}>
        {drag.announcement || announcement}
      </span>
    </>
  );
}

export type CollectionProps = {
  items: $ReadOnlyArray<CollectionItem>,
  children?: (item: CollectionItem, state: CollectionItemState) => React.Node,
  selectionMode?: SelectionMode,
  /**
   * What a plain click or Space does: `"toggle"` flips the row (the default),
   * `"replace"` makes it the whole selection and lets the arrow keys carry the
   * selection with them. Ctrl/Cmd toggles and Shift extends under either.
   */
  selectionBehavior?: SelectionBehavior,
  /** Refuse the gesture that would leave nothing selected. */
  disallowEmptySelection?: boolean,
  /** Whether Escape clears the selection. */
  escapeKeyBehavior?: "clearSelection" | "none",
  selectedKeys?: $ReadOnlyArray<string>,
  defaultSelectedKeys?: $ReadOnlyArray<string>,
  onSelectionChange?: (keys: $ReadOnlyArray<string>) => void,
  /**
   * Open a row rather than select it. Enter runs it; so does a click when
   * `selectionMode` is `"none"`, and a double click under
   * `selectionBehavior="replace"`.
   */
  onAction?: (key: string) => void,
  disabledKeys?: $ReadOnlyArray<string>,
  expandedKeys?: $ReadOnlyArray<string>,
  defaultExpandedKeys?: $ReadOnlyArray<string>,
  onExpandedChange?: (keys: $ReadOnlyArray<string>) => void,
  onRemove?: (key: string) => void,
  onReorder?: (keys: $ReadOnlyArray<string>) => void,
  loading?: boolean,
  onLoadMore?: () => void,
  virtualized?: boolean,
  height?: number,
  rowHeight?: number,
  render?: RenderProp,
  readonly key?: empty,
  readonly [string]: mixed,
};
