// @flow
import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { afterEach, describe, expect, fn, it } from "@uniflowed/test";
import { cleanup, fireEvent, render, screen, userEvent } from "@uniflowed/react-testing";
import { GridList, I18nProvider, ListBox, TagGroup, Tree } from "./index.js";

afterEach(cleanup);
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
  list.focus();
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
  list.focus();
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
  tree.focus();
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
  grid.focus();
  await userEvent.keyboard("{Delete}");
  expect(screen.queryByText("Apple")).toBe(null);
  expect(screen.getByRole("status").textContent).toBe("Removed Apple");
  expect(
    document.getElementById(grid.getAttribute("aria-activedescendant"))?.textContent,
  ).toContain("Cherry");
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
