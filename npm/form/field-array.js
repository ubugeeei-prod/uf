// @flow
//
// `@uniflowed/form/field-array`: a list of rows, each of which is fields.
//
//   const { fields, append, remove } = useFieldArray({ control, name: "items" });
//
//   {fields.map((field, index) => (
//     <li key={field.id}>
//       <input {...register(`items.${index}.name`)} />
//       <button type="button" onClick={() => remove(index)}>Remove</button>
//     </li>
//   ))}
//
// # The keys are the whole hook
//
// `field.id` is made once, when the row is created, and travels with the row
// through every `remove`, `insert`, `swap` and `move`. It is never the index.
//
// That is not a React nicety, it is the difference between working and not. The
// inputs are uncontrolled: their text lives in the DOM, not in a prop. Key the
// rows by index and removing the middle of three rows leaves React looking at
// two rows whose keys are `0` and `1` where there were three whose keys were
// `0`, `1` and `2` — so it keeps the first two DOM nodes and drops the third,
// and the row that survives is showing the *removed* row's text with the
// *surviving* row's name. Nothing throws. The form just submits the wrong data.
//
// With stable keys React removes the node that belongs to the row that went and
// the rows that shifted keep theirs. Their field *names* change, though —
// `items.2.name` becomes `items.1.name` — so the operation rewrites everything
// the form knew about those rows by index: the errors, the dirty flags and the
// touched flags. That rewrite is in `internal/form-store.js`, and because all
// three are keyed by path it is a rename of strings rather than a walk of the
// values.
//
// # Why `fields` is a snapshot and not the values
//
// `fields` is `[{ id, ...row }]` — the row's values with its key attached — and
// it is cached in the store so that its identity only moves when the array
// does. `useSyncExternalStore` compares that identity, so a keystroke inside
// row 3 does not re-render a component that only rendered the list.
//
// A row's *values* in `fields` are the values as of the last array operation,
// not as of the last keystroke, and that is deliberate: an uncontrolled row's
// current text is in the DOM, and re-rendering the list on every character to
// keep a copy in sync would undo the point of the library. Read the live value
// with `useWatch({ control, name: "items.3.price" })` where one is needed, or
// `getValues()` at submit time, where all of them are.

import { useCallback, useMemo, useSyncExternalStore } from "@uniflowed/react";

import type { FieldPath, FieldValues } from "./internal/field-path.js";
import type { Control, FieldArrayRow } from "./internal/form-store.js";

export type { FieldArrayRow };

export type UseFieldArrayOptions<TValues extends FieldValues, TOutput> = {|
  readonly control: Control<TValues, TOutput>,
  readonly name: FieldPath,
|};

export type UseFieldArrayReturn = {|
  /** The rows, each with a stable `id`. Map over these, not over the values. */
  readonly fields: $ReadOnlyArray<FieldArrayRow>,
  /** Add to the end. */
  readonly append: (value: mixed | $ReadOnlyArray<mixed>) => void,
  /** Add to the front. */
  readonly prepend: (value: mixed | $ReadOnlyArray<mixed>) => void,
  /** Add at `index`, pushing the rest along. */
  readonly insert: (index: number, value: mixed | $ReadOnlyArray<mixed>) => void,
  /** Remove one row, several, or — with no argument — all of them. */
  readonly remove: (index?: number | $ReadOnlyArray<number>) => void,
  /** Exchange two rows, keeping each one's key with its values. */
  readonly swap: (left: number, right: number) => void,
  /** Take a row out and put it back at another index. */
  readonly move: (from: number, to: number) => void,
  /** Replace one row's values, keeping its key: it is still the same row. */
  readonly update: (index: number, value: mixed) => void,
  /** Replace every row. All the keys are new, because none of the rows are. */
  readonly replace: (values: $ReadOnlyArray<mixed>) => void,
|};

/** Namespaces this hook's subscription so it cannot collide with a `useWatch`. */
const PREFIX = "fields.";

/**
 * Rows for the array at `name`.
 *
 * The list re-renders when the array's *shape* changes — a row added, removed,
 * moved — and when a value inside it is written through `setValue` or `update`.
 * It does not re-render when somebody types into one of the rows, because the
 * text is in the DOM and nothing in this list is showing it.
 */
export hook useFieldArray<TValues extends FieldValues, TOutput>(
  options: UseFieldArrayOptions<TValues, TOutput>,
): UseFieldArrayReturn {
  const control = options.control;
  const name = options.name;

  const paths = useMemo(() => [name], [name]);
  const key = `${PREFIX}${name}`;

  const subscribe = useCallback(
    (listener: () => void) => control.subscribeWatch(key, paths, listener),
    [control, key, paths],
  );
  // The rows are built and cached by the store, not by a `useMemo` here. The
  // reason is in `internal/form-store.js` under `arrayRows`, and it is the one
  // thing about this hook that is not obvious: the React Compiler will hold a
  // `useMemo` over a store read for ever, because it can see nothing that says
  // the store changed. `useSyncExternalStore` is what says so.
  const snapshot = useCallback(() => control.arrayRows(name), [control, name]);
  const fields = useSyncExternalStore(subscribe, snapshot, snapshot);

  const operations = useMemo(() => {
    const length = (): number => {
      const held = control.valueAt(name);
      return Array.isArray(held) ? held.length : 0;
    };
    const many = (value: mixed | $ReadOnlyArray<mixed>): $ReadOnlyArray<mixed> =>
      Array.isArray(value) ? value : [value];

    return {
      append: (value: mixed | $ReadOnlyArray<mixed>) =>
        control.spliceArray(name, length(), 0, many(value)),
      prepend: (value: mixed | $ReadOnlyArray<mixed>) =>
        control.spliceArray(name, 0, 0, many(value)),
      insert: (index: number, value: mixed | $ReadOnlyArray<mixed>) =>
        control.spliceArray(name, index, 0, many(value)),
      remove: (index?: number | $ReadOnlyArray<number>) => {
        if (index == null) {
          control.replaceArray(name, []);
          return;
        }
        // Highest index first, so each splice leaves the ones below it alone.
        const targets = (typeof index === "number" ? [index] : Array.from(index)).sort(
          (left, right) => right - left,
        );
        for (const target of targets) {
          control.spliceArray(name, target, 1, []);
        }
      },
      swap: (left: number, right: number) => control.swapArray(name, left, right),
      move: (from: number, to: number) => control.moveArray(name, from, to),
      update: (index: number, value: mixed) => control.updateArray(name, index, value),
      replace: (next: $ReadOnlyArray<mixed>) => control.replaceArray(name, next),
    };
  }, [control, name]);

  return useMemo(() => ({ fields, ...operations }), [fields, operations]);
}
