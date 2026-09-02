// @flow
//
// `@uniflowed/state`: the cell primitives plus their React binding.

import type { Cell, CellScope } from "@uniflowed/cell";
import { cell, computed, read, resource, subscribe, update, write } from "@uniflowed/cell";

// $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
export opaque type Atom<T> = Cell<T>;
export opaque type ReadonlyAtom<T> = Cell<T>;
export type AtomSetter<T> = (next: T | ((current: T) => T)) => void;
export type AtomTuple<T> = [T, AtomSetter<T>];
export type StorageAdapter = {|
  readonly getItem: (key: string) => null | string,
  readonly setItem: (key: string, value: string) => void,
|};

export type { Cell, CellScope };
export { cell, computed, read, resource, subscribe, update, write };

export function atom<T>(initial: T): Atom<T> {
  return cell(initial);
}

export function selector<T>(derive: () => T): ReadonlyAtom<T> {
  return computed(derive);
}

export function atomWithStorage<T>(key: string, initial: T, storage?: StorageAdapter): Atom<T> {
  if (storage == null) {
    return atom(initial);
  }
  const stored = storage.getItem(key);
  const atomSource = atom(stored == null ? initial : JSON.parse(stored));
  subscribe(atomSource, () => {
    storage.setItem(key, JSON.stringify(read(atomSource)));
  });
  return atomSource;
}

function applyAtomUpdate<T>(source: Atom<T>, next: T | ((current: T) => T)): void {
  if (typeof next === "function") {
    // Flow cannot refine callable union payloads here yet, so the dynamic updater
    // boundary lives in one helper instead of leaking into every hook consumer.
    // $FlowFixMe[incompatible-use]
    update(source, next as (T) => T);
    return;
  }
  write(source, next);
}

export hook useCell<T>(
  // $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
  source: Cell<T>,
): T {
  return read(source);
}

export hook useAtom<T>(source: Atom<T>): AtomTuple<T> {
  const set: AtomSetter<T> = (next) => {
    applyAtomUpdate(source, next);
  };
  return [read(source), set];
}
