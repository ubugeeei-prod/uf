// @flow
"use client";

import * as React from "@uniflowed/react";
import { useId, useRef, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";
import { useDragAndDrop } from "../drag-drop.js";
import { useControlled } from "./controlled-state.js";
import { composeHandlers, composeRefs, withProps } from "./merge-props.js";
import type { RenderProp, Rest } from "./merge-props.js";
import { directionOf } from "./roving-focus.js";
import { startsWithLocale, useLocale } from "../i18n-provider.js";

export type CollectionItem = {
  readonly key: string,
  readonly textValue: string,
  readonly disabled?: boolean,
  readonly children?: $ReadOnlyArray<CollectionItem>,
};
type Entry = {
  item: CollectionItem,
  level: number,
  parent: string | null,
  position: number,
  count: number,
};
type Kind = "listbox" | "grid" | "tree" | "tags";

function entries(
  items: $ReadOnlyArray<CollectionItem>,
  expanded: $ReadOnlyArray<string>,
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
      if (tree && children != null && expanded.includes(item.key))
        visit(children, level + 1, item.key);
    });
  };
  visit(items, 1, null);
  return result;
}

/** Data owns order and identity. Only visible tree descendants and virtual rows enter the DOM. */
export component CollectionRoot(kind: Kind, options: CollectionProps) {
  const {
    items,
    children,
    selectionMode: requestedSelectionMode = "single",
    selectedKeys,
    defaultSelectedKeys = [],
    onSelectionChange,
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
  const selectionMode = kind === "tags" ? "none" : requestedSelectionMode;
  if (height <= 0 || rowHeight <= 0 || !Number.isFinite(height) || !Number.isFinite(rowHeight))
    throw new RangeError("Collection dimensions must be positive finite numbers");
  const { locale } = useLocale();
  const id = useId();
  const root = useRef<HTMLElement | null>(null);
  const anchor = useRef<string | null>(null);
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
  const drag = useDragAndDrop({
    disabled: onReorder == null,
    onDrop: ({ keys, target }) => {
      const moving = keys[0];
      if (
        [moving, target].some(
          (key) => disabledKeys.includes(key) || items.find((item) => item.key === key)?.disabled,
        )
      )
        return;
      if (
        moving === target ||
        !items.some((item) => item.key === moving) ||
        !items.some((item) => item.key === target)
      )
        return;
      const ordered = items.filter((item) => item.key !== moving).map((item) => item.key);
      ordered.splice(ordered.indexOf(target), 0, moving);
      onReorder?.(ordered);
    },
  });
  const rows = entries(items, expanded, kind === "tree");
  const disabled = (item: CollectionItem) =>
    item.disabled === true || disabledKeys.includes(item.key);
  const enabled = rows.filter((row) => !disabled(row.item));
  const focused =
    enabled.find((row) => row.item.key === active) ??
    enabled.find((row) => selected.includes(row.item.key)) ??
    enabled[0];
  const activeKey = focused?.item.key;
  const choose = (key: string, range: boolean, toggle: boolean) => {
    const item = rows.find((row) => row.item.key === key)?.item;
    if (item == null || disabled(item) || selectionMode === "none") return;
    let next;
    if (selectionMode === "single") next = [key];
    else if (range && anchor.current != null) {
      const from = enabled.findIndex((row) => row.item.key === anchor.current);
      const to = enabled.findIndex((row) => row.item.key === key);
      next = enabled
        .slice(Math.min(Math.max(from, 0), to), Math.max(from, to) + 1)
        .map((row) => row.item.key);
    } else {
      next = toggle
        ? selected.includes(key)
          ? selected.filter((each) => each !== key)
          : [...selected, key]
        : [key];
      anchor.current = key;
    }
    setSelected(next);
    announce(`${next.length} selected`);
  };
  const land = (row: Entry, shift: boolean) => {
    setActive(row.item.key);
    const container = root.current;
    if (virtualized && container != null) {
      const top = rows.indexOf(row) * rowHeight;
      if (top < container.scrollTop || top + rowHeight > container.scrollTop + height) {
        container.scrollTop = top;
        setScrollTop(top);
      }
    }
    if (shift) choose(row.item.key, true, false);
  };
  const remove = (key: string) => {
    const at = enabled.findIndex((row) => row.item.key === key);
    if (at < 0 || onRemove == null) return;
    onRemove(key);
    setActive(enabled[at + 1]?.item.key ?? enabled[at - 1]?.item.key ?? null);
    announce(`Removed ${enabled[at].item.textValue}`);
  };
  const keydown = useStableCallback((event: $FlowFixMe) => {
    if (
      event.target !== event.currentTarget &&
      event.target.closest("button,input,textarea,select")
    )
      return;
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
    if (
      onReorder != null &&
      event.key === " " &&
      (event.ctrlKey || event.metaKey) &&
      focused != null
    ) {
      event.preventDefault();
      drag.start(focused.item.key, focused.item.textValue);
      return;
    }
    const at = enabled.findIndex((row) => row.item.key === activeKey);
    let next = null;
    if (event.key === "ArrowDown") next = enabled[Math.min(at + 1, enabled.length - 1)];
    else if (event.key === "ArrowUp") next = enabled[Math.max(at - 1, 0)];
    else if (event.key === "Home") next = enabled[0];
    else if (event.key === "End") {
      next = enabled[enabled.length - 1];
      if (!loading) onLoadMore?.();
    } else if (
      kind === "tree" &&
      focused != null &&
      ["ArrowLeft", "ArrowRight"].includes(event.key)
    ) {
      const open =
        event.key === (directionOf(event.currentTarget) === "rtl" ? "ArrowLeft" : "ArrowRight");
      const key = focused.item.key;
      if (open && focused.item.children?.length) {
        if (!expanded.includes(key)) setExpanded([...expanded, key]);
        else next = enabled[at + 1];
      } else if (!open && expanded.includes(key))
        setExpanded(expanded.filter((each) => each !== key));
      else if (!open) next = enabled.find((row) => row.item.key === focused.parent);
    } else if (
      (event.ctrlKey || event.metaKey) &&
      event.key === "a" &&
      selectionMode === "multiple"
    ) {
      setSelected(enabled.map((row) => row.item.key));
      announce(`${enabled.length} selected`);
    } else if ((event.key === " " || event.key === "Enter") && activeKey != null)
      choose(activeKey, event.shiftKey, true);
    else if (
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
    } else return;
    event.preventDefault();
    if (next != null) land(next, event.shiftKey);
  });
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
      const chosen = selected.includes(item.key);
      const grid = kind === "grid" || kind === "tags";
      const content = children?.(item) ?? item.textValue;
      const props = {
        ...drag.getDragProps(item.key, item.textValue),
        ...drag.getDropProps(item.key, item.textValue),
        onKeyDown: undefined,
        draggable: onReorder != null && !disabled(item),
        id: `${id}-${index}`,
        role: grid ? "row" : kind === "tree" ? "treeitem" : "option",
        "aria-selected": selectionMode === "none" ? undefined : chosen,
        "aria-disabled": disabled(item) || undefined,
        "aria-expanded":
          kind === "tree" && item.children?.length ? expanded.includes(item.key) : undefined,
        "aria-level": kind === "tree" ? row.level : undefined,
        "aria-posinset": grid ? undefined : kind === "tree" ? row.position : index + 1,
        "aria-setsize": grid ? undefined : kind === "tree" ? row.count : rows.length,
        "aria-rowindex": grid ? index + 1 : undefined,
        "data-key": item.key,
        style: virtualized
          ? { position: "absolute", top: index * rowHeight, height: rowHeight, width: "100%" }
          : undefined,
        onClick: (event: $FlowFixMe) => {
          if (disabled(item)) return;
          root.current?.focus();
          setActive(item.key);
          choose(item.key, event.shiftKey, selectionMode === "multiple");
        },
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
                  aria-label={`Remove ${item.textValue}`}
                  onClick={(event) => {
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
  const props = withProps(rest, {
    ref: composeRefs(rest.ref, setRef),
    role: kind === "tags" ? "grid" : kind,
    tabIndex: 0,
    "aria-activedescendant": activeIndex < 0 ? undefined : `${id}-${activeIndex}`,
    "aria-multiselectable": selectionMode === "multiple" || undefined,
    "aria-busy": loading || undefined,
    "aria-rowcount": kind === "grid" || kind === "tags" ? rows.length : undefined,
    onKeyDown: composeHandlers(rest.onKeyDown, keydown),
    onScroll: composeHandlers(rest.onScroll, (event: $FlowFixMe) => {
      if (virtualized) setScrollTop(event.currentTarget.scrollTop);
      if (
        !loading &&
        event.currentTarget.scrollTop + event.currentTarget.clientHeight >=
          event.currentTarget.scrollHeight
      )
        onLoadMore?.();
    }),
    style: virtualized
      ? { ...(rest.style as $FlowFixMe), height, overflow: "auto", position: "relative" }
      : rest.style,
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
      <span role="status" aria-live="polite">
        {drag.announcement || announcement}
      </span>
    </>
  );
}

export type CollectionProps = {
  items: $ReadOnlyArray<CollectionItem>,
  children?: (item: CollectionItem) => React.Node,
  selectionMode?: "single" | "multiple" | "none",
  selectedKeys?: $ReadOnlyArray<string>,
  defaultSelectedKeys?: $ReadOnlyArray<string>,
  onSelectionChange?: (keys: $ReadOnlyArray<string>) => void,
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
