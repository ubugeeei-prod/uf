// @flow
import * as React from "@uniflowed/react";
import { StrictMode, useState } from "@uniflowed/react";
import { afterEach, describe, expect, fn, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";
import { GridList, I18nProvider, ListBox, TagGroup, Tree } from "./index.js";
import {
  clearAll,
  extendTo,
  keyRange,
  orderedKeys,
  replaceWith,
  selectAll,
  toggleKey,
} from "./internal/selection.js";

afterEach(cleanup);

/** Focus what a query found; a query answers `Element`, which has no `focus`. */
function focusOn(element: Element): void {
  if (!(element instanceof HTMLElement)) throw new Error("expected an HTML element");
  element.focus();
}

function activeName(list: Element): string | void {
  const id = list.getAttribute("aria-activedescendant");
  return id == null ? undefined : (document.getElementById(id)?.textContent ?? undefined);
}
const items = [
  { key: "a", textValue: "Apple" },
  { key: "b", textValue: "Banana", disabled: true },
  { key: "c", textValue: "Cherry" },
];

it("selects shift ranges and skips disabled keys", async () => {
  const change = fn();
  render(
    <ListBox
      items={items}
      aria-label="Fruit"
      selectionMode="multiple"
      onSelectionChange={change}
    />,
  );
  const list = screen.getByRole("listbox");
  focusOn(list);
  await userEvent.keyboard(" ");
  fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
  expect(change).toHaveBeenLastCalledWith(["a", "c"]);
  expect(screen.getByRole("option", { name: "Banana" }).getAttribute("aria-selected")).toBe(
    "false",
  );
});

it("keeps the active descendant mounted among 10,000 virtual rows", async () => {
  render(
    <ListBox
      aria-label="Logs"
      items={Array.from({ length: 10000 }, (_, i) => ({ key: String(i), textValue: `Line ${i}` }))}
      virtualized
    />,
  );
  const list = screen.getByRole("listbox");
  expect(screen.getAllByRole("option").length).toBeLessThan(20);
  focusOn(list);
  await userEvent.keyboard("{End}");
  const last = screen.getByRole("option", { name: "Line 9999" });
  expect(list.getAttribute("aria-activedescendant")).toBe(last.id);
  expect(last.getAttribute("aria-posinset")).toBe("10000");
  expect(screen.getAllByRole("option").length).toBeLessThan(20);
});

it("expands and collapses a tree with Arabic arrow direction", async () => {
  render(
    <I18nProvider locale="ar">
      <Tree
        aria-label="Files"
        items={[{ key: "p", textValue: "Parent", children: [{ key: "c", textValue: "Child" }] }]}
      />
    </I18nProvider>,
  );
  const tree = screen.getByRole("tree");
  focusOn(tree);
  await userEvent.keyboard("{ArrowLeft}");
  expect(screen.getByRole("treeitem", { name: "Parent" }).getAttribute("aria-expanded")).toBe(
    "true",
  );
  await userEvent.keyboard("{ArrowLeft}");
  expect(tree.getAttribute("aria-activedescendant")).toBe(
    screen.getByRole("treeitem", { name: "Child" }).id,
  );
  await userEvent.keyboard("{ArrowRight}{ArrowRight}");
  expect(screen.queryByRole("treeitem", { name: "Child" })).toBe(null);
});

component Tags() {
  const [tags, setTags] = useState(items);
  return (
    <TagGroup
      aria-label="Tags"
      items={tags}
      onRemove={(key) => setTags(tags.filter((item) => item.key !== key))}
    />
  );
}
it("removes tags by keyboard and announces the removal", async () => {
  render(<Tags />);
  const grid = screen.getByRole("grid");
  focusOn(grid);
  await userEvent.keyboard("{Delete}");
  expect(screen.queryByText("Apple")).toBe(null);
  expect(screen.getByRole("status").textContent).toBe("Removed Apple");
  expect(activeName(grid)).toContain("Cherry");
});

it("audits all four collection surfaces with axe", async () => {
  const { container } = render(
    <main>
      <h1>Collections</h1>
      <ListBox items={items} aria-label="Fruit" />
      <GridList items={items} aria-label="Grid" />
      <Tree items={items} aria-label="Files" />
      <TagGroup items={items} aria-label="Tags" />
    </main>,
  );
  await expect(container).toHaveNoAxeViolations();
});

// ---------------------------------------------------------------------------
// The selection model: `internal/selection.js`, and the keys and pointers that
// reach it. React Aria's selection manager is the reference for every rule.
// ---------------------------------------------------------------------------

const fruit = [
  { key: "a", textValue: "Apple" },
  { key: "b", textValue: "Banana" },
  { key: "c", textValue: "Cherry" },
  { key: "d", textValue: "Damson" },
];

/** The keys an `onSelectionChange` mock was last called with. */
function lastKeys(mock: {
  readonly mock: { readonly lastCall: $ReadOnlyArray<mixed> | void, ... },
  ...
}): mixed {
  return mock.mock.lastCall?.[0];
}

describe("selection rules", () => {
  const toggle = { mode: "multiple", behavior: "toggle", disallowEmpty: false } as const;
  const single = { mode: "single", behavior: "toggle", disallowEmpty: false } as const;

  it("toggles a single selection off unless empty is disallowed", () => {
    const one = new Set(["a"]);
    expect([...toggleKey(single, one, "a")]).toEqual([]);
    expect(toggleKey({ ...single, disallowEmpty: true }, one, "a")).toBe(one);
    expect([...toggleKey(single, one, "b")]).toEqual(["b"]);
  });

  it("shrinks a Shift range back towards its anchor instead of growing it", () => {
    const order = ["a", "b", "c", "d"];
    // Shift+Down twice from a, then Shift+Up once: a…c, then a…b.
    const two = extendTo(toggle, new Set(["a"]), order, "a", "a", "c");
    expect(orderedKeys(two, order)).toEqual(["a", "b", "c"]);
    const back = extendTo(toggle, two, order, "a", "c", "b");
    expect(orderedKeys(back, order)).toEqual(["a", "b"]);
  });

  it("keeps a Ctrl-chosen key outside the range and skips keys not in the order", () => {
    const order = ["a", "c", "d"]; // b is disabled, so it is not in the order
    const next = extendTo(toggle, new Set(["d"]), order, "a", null, "c");
    expect(orderedKeys(next, order)).toEqual(["a", "c", "d"]);
    expect(keyRange(order, "a", "c")).toEqual(["a", "c"]);
    expect(keyRange(order, "gone", "c")).toEqual(["c"]);
  });

  it("returns the same set for a gesture that changes nothing", () => {
    const all = new Set(["a", "b"]);
    expect(selectAll(toggle, all, ["a", "b"])).toBe(all);
    expect(clearAll({ ...toggle, disallowEmpty: true }, all)).toBe(all);
    expect(replaceWith(toggle, new Set(["a"]), "a").size).toBe(1);
  });

  it("reports hidden keys after the visible ones rather than dropping them", () => {
    expect(orderedKeys(new Set(["hidden", "c", "a"]), ["a", "b", "c"])).toEqual([
      "a",
      "c",
      "hidden",
    ]);
  });
});

describe("selectionBehavior replace", () => {
  it("carries the selection with the arrow keys, and Ctrl moves focus alone", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        selectionBehavior="replace"
        onSelectionChange={change}
      />,
    );
    const list = screen.getByRole("listbox");
    focusOn(list);
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(lastKeys(change)).toEqual(["b"]);
    fireEvent.keyDown(list, { key: "ArrowDown", ctrlKey: true });
    expect(activeName(list)).toBe("Cherry");
    expect(change).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(list, { key: " ", ctrlKey: true });
    expect(lastKeys(change)).toEqual(["b", "c"]);
    fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
    expect(lastKeys(change)).toEqual(["b", "c", "d"]);
  });

  it("replaces on click, toggles on Ctrl/Cmd+click and extends on Shift+click", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        selectionBehavior="replace"
        onSelectionChange={change}
      />,
    );
    const option = (name: string) => screen.getByRole("option", { name });
    fireEvent.click(option("Apple"), { detail: 1 });
    expect(lastKeys(change)).toEqual(["a"]);
    fireEvent.click(option("Cherry"), { detail: 1 });
    expect(lastKeys(change)).toEqual(["c"]);
    fireEvent.click(option("Apple"), { detail: 1, metaKey: true });
    expect(lastKeys(change)).toEqual(["a", "c"]);
    fireEvent.click(option("Damson"), { detail: 1, shiftKey: true });
    expect(lastKeys(change)).toEqual(["a", "b", "c", "d"]);
  });

  it("toggles on a virtual click, which has no modifier to hold", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        selectionBehavior="replace"
        defaultSelectedKeys={["a"]}
        onSelectionChange={change}
      />,
    );
    // `detail: 0` is a click no pointer made — a screen reader's activation.
    fireEvent.click(screen.getByRole("option", { name: "Cherry" }), { detail: 0 });
    expect(lastKeys(change)).toEqual(["a", "c"]);
  });

  it("runs onAction on a double click and on Enter, not on a single click", () => {
    const action = fn();
    const change = fn();
    render(
      <GridList
        aria-label="Files"
        items={fruit}
        selectionMode="multiple"
        selectionBehavior="replace"
        onAction={action}
        onSelectionChange={change}
      />,
    );
    const row = screen.getAllByRole("row")[1];
    fireEvent.click(row, { detail: 1 });
    expect(action).not.toHaveBeenCalled();
    expect(lastKeys(change)).toEqual(["b"]);
    fireEvent.dblClick(row, { detail: 2 });
    expect(action).toHaveBeenLastCalledWith("b");
    fireEvent.keyDown(screen.getByRole("grid"), { key: "Enter" });
    expect(action).toHaveBeenCalledTimes(2);
  });
});

describe("empty selection and Escape", () => {
  it("deselects a single selection on Space unless empty selection is disallowed", () => {
    const change = fn();
    const { unmount } = render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        defaultSelectedKeys={["a"]}
        onSelectionChange={change}
      />,
    );
    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: " " });
    expect(lastKeys(change)).toEqual([]);
    unmount();

    const kept = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        defaultSelectedKeys={["a"]}
        disallowEmptySelection
        onSelectionChange={kept}
      />,
    );
    fireEvent.keyDown(screen.getByRole("listbox"), { key: " " });
    expect(kept).not.toHaveBeenCalled();
    expect(screen.getByRole("option", { name: "Apple" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });

  it("clears on Escape, and leaves Escape unclaimed when there is nothing to clear", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        defaultSelectedKeys={["a", "c"]}
        onSelectionChange={change}
      />,
    );
    const list = screen.getByRole("listbox");
    // `fireEvent` answers whether the event went unprevented.
    expect(fireEvent.keyDown(list, { key: "Escape" })).toBe(false);
    expect(lastKeys(change)).toEqual([]);
    expect(fireEvent.keyDown(list, { key: "Escape" })).toBe(true);
    expect(change).toHaveBeenCalledTimes(1);
  });

  it("keeps the selection on Escape when escapeKeyBehavior is none", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        defaultSelectedKeys={["a"]}
        escapeKeyBehavior="none"
        onSelectionChange={change}
      />,
    );
    expect(fireEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" })).toBe(true);
    expect(change).not.toHaveBeenCalled();
  });
});

describe("paging, ranges and select-all", () => {
  const lines = Array.from({ length: 100 }, (_, i) => ({ key: String(i), textValue: `Line ${i}` }));

  it("moves a viewport's worth of rows on PageDown and PageUp", () => {
    render(<ListBox aria-label="Logs" items={lines} virtualized height={300} rowHeight={30} />);
    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "PageDown" });
    expect(activeName(list)).toBe("Line 10");
    fireEvent.keyDown(list, { key: "PageDown" });
    expect(activeName(list)).toBe("Line 20");
    fireEvent.keyDown(list, { key: "PageUp" });
    expect(activeName(list)).toBe("Line 10");
  });

  it("extends to the end with Shift+End and takes everything with Ctrl+Shift+A", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        onSelectionChange={change}
      />,
    );
    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "End", shiftKey: true });
    expect(lastKeys(change)).toEqual(["b", "c", "d"]);
    fireEvent.keyDown(list, { key: "Escape" });
    // Caps lock or Shift turns the key into "A"; it is still select-all.
    fireEvent.keyDown(list, { key: "A", ctrlKey: true, shiftKey: true });
    expect(lastKeys(change)).toEqual(["a", "b", "c", "d"]);
  });

  it("runs onAction on Enter and selects on Space", () => {
    const action = fn();
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        onAction={action}
        onSelectionChange={change}
      />,
    );
    const list = screen.getByRole("listbox");
    fireEvent.keyDown(list, { key: "Enter" });
    expect(action).toHaveBeenLastCalledWith("a");
    expect(change).not.toHaveBeenCalled();
    fireEvent.keyDown(list, { key: " " });
    expect(lastKeys(change)).toEqual(["a"]);
  });

  it("runs onAction on click when there is nothing to select", () => {
    const action = fn();
    render(<GridList aria-label="Links" items={fruit} selectionMode="none" onAction={action} />);
    fireEvent.click(screen.getByText("Cherry"), { detail: 1 });
    expect(action).toHaveBeenLastCalledWith("c");
  });

  it("does not move a controlled selection the parent refused", () => {
    const change = fn();
    render(
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        selectedKeys={["a"]}
        onSelectionChange={change}
      />,
    );
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" });
    expect(lastKeys(change)).toEqual([]);
    expect(screen.getByRole("option", { name: "Apple" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });
});

describe("tags and trees", () => {
  it("walks a tag group with the horizontal arrows, mirrored right to left", () => {
    const { unmount } = render(<TagGroup aria-label="Tags" items={fruit} />);
    let grid = screen.getByRole("grid");
    fireEvent.keyDown(grid, { key: "ArrowRight" });
    expect(activeName(grid)).toBe("Banana");
    fireEvent.keyDown(grid, { key: "ArrowLeft" });
    expect(activeName(grid)).toBe("Apple");
    unmount();

    render(
      <I18nProvider locale="he">
        <TagGroup aria-label="Tags" items={fruit} />
      </I18nProvider>,
    );
    grid = screen.getByRole("grid");
    fireEvent.keyDown(grid, { key: "ArrowLeft" });
    expect(activeName(grid)).toBe("Banana");
  });

  it("expands every sibling with * and nothing else", () => {
    render(
      <Tree
        aria-label="Files"
        items={[
          {
            key: "src",
            textValue: "src",
            children: [
              { key: "lib", textValue: "lib", children: [{ key: "x", textValue: "x.js" }] },
            ],
          },
          { key: "docs", textValue: "docs", children: [{ key: "y", textValue: "y.md" }] },
          { key: "readme", textValue: "README" },
        ]}
      />,
    );
    fireEvent.keyDown(screen.getByRole("tree"), { key: "*" });
    expect(screen.getByRole("treeitem", { name: "src" }).getAttribute("aria-expanded")).toBe(
      "true",
    );
    expect(screen.getByRole("treeitem", { name: "docs" }).getAttribute("aria-expanded")).toBe(
      "true",
    );
    // A child's own children stay closed: * is one level, not the whole tree.
    expect(screen.getByRole("treeitem", { name: "lib" }).getAttribute("aria-expanded")).toBe(
      "false",
    );
  });
});

it("reports each gesture once under Strict Mode", () => {
  const change = fn();
  render(
    <StrictMode>
      <ListBox
        aria-label="Fruit"
        items={fruit}
        selectionMode="multiple"
        selectionBehavior="replace"
        onSelectionChange={change}
      />
    </StrictMode>,
  );
  const list = screen.getByRole("listbox");
  fireEvent.keyDown(list, { key: "ArrowDown" });
  fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
  expect(change.mock.calls.map((args) => args[0])).toEqual([["b"], ["b", "c"]]);
});
