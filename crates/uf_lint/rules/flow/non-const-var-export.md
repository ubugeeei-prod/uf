An exported `let` or `var` can be reassigned by the module after other modules have imported it, and the importers see the change through a live binding. Export a `const`, and export a function when callers need to change the value.

## Bad

```js
// @flow
export let count = 0;
```

```diagnostics
app/example.js:2:8 exported bindings must be `const`; a mutable export is a live binding
```

## Good

```js
// @flow
let count = 0;

export const initial = 0;

export function increment(): number {
  count += 1;
  return count;
}
```
