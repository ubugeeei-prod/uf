"use client";
// @flow
import * as React from "react";
import { useState, useEffect, useCallback } from "react";
import type { Node } from "react";

type Props = {|
  +title: string,
  +items: $ReadOnlyArray<{| +id: string, +label: string |}>,
  +onSelect?: (id: string) => void,
|};

export component ItemList(
  title: string,
  items: $ReadOnlyArray<{| +id: string, +label: string |}>,
  onSelect?: (id: string) => void,
) renders Node {
  const [selected, setSelected] = useState<?string>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (selected != null) {
      onSelect?.(selected);
    }
  }, [selected, onSelect]);

  const handleChange = useCallback((event: SyntheticInputEvent<HTMLInputElement>) => {
    setQuery(event.target.value);
  }, []);

  const visible = items.filter((item) => item.label.toLowerCase().includes(query.toLowerCase()));

  if (visible.length === 0) {
    return <p className="empty">No items match “{query}”.</p>;
  }

  return (
    <section className="item-list" aria-label={title}>
      <h2>{title}</h2>
      <input type="search" value={query} onChange={handleChange} placeholder="Filter items" />
      <ul>
        {visible.map((item) => (
          <li
            key={item.id}
            className={item.id === selected ? "selected" : undefined}
            onClick={() => setSelected(item.id)}
          >
            {item.label}
          </li>
        ))}
      </ul>
      {selected != null && <footer>Selected: {selected}</footer>}
    </section>
  );
}

export hook useToggle(initial: boolean = false): [boolean, () => void] {
  const [value, setValue] = useState(initial);
  const toggle = useCallback(() => setValue((previous) => !previous), []);
  return [value, toggle];
}

export default function App(): Node {
  const [open, toggle] = useToggle();
  return (
    <>
      <button onClick={toggle}>{open ? "Close" : "Open"}</button>
      {open ? <ItemList title="Things" items={[]} /> : null}
    </>
  );
}
