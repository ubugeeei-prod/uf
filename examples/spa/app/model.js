// @flow
import type { Atom, ReadonlyAtom, WriteOnlyAtom } from "@uniflowed/state";
import { action, atom, selector } from "@uniflowed/state";

/** One task shared by the board and the progress page. */
export type Task = {
  readonly id: number,
  readonly title: string,
  readonly done: boolean,
};

/** Client memory persists through navigation and resets on a reload. */
export const tasks: Atom<$ReadOnlyArray<Task>> = atom<$ReadOnlyArray<Task>>([
  { id: 1, title: "Write a Flow component", done: true },
  { id: 2, title: "Try navigating to Progress", done: false },
]);

/** Completion count derived from the current board. */
export const completed: ReadonlyAtom<number> = selector<number>(
  (get) => get(tasks).filter((task) => task.done).length,
);

/** Trim new tasks and assign an identifier within the current store. */
export const addTask: WriteOnlyAtom<string> = action<string>((get, set, input) => {
  const title = input.trim();
  if (title.length === 0) return;
  const current = get(tasks);
  const id = current.reduce((largest, task) => Math.max(largest, task.id), 0) + 1;
  set(tasks, [...current, { id, title, done: false }]);
});

/** Toggle one task without mutating another component's snapshot. */
export const toggleTask: WriteOnlyAtom<number> = action<number>((get, set, id) => {
  set(
    tasks,
    get(tasks).map((task) => (task.id === id ? { ...task, done: !task.done } : task)),
  );
});

/** Remove completed tasks from the shared board. */
export const clearCompleted: WriteOnlyAtom<void> = action<void>((get, set) => {
  set(
    tasks,
    get(tasks).filter((task) => !task.done),
  );
});
