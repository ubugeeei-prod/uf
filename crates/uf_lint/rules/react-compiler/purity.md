Rendering must be pure: the same props and state give the same output. `Date.now()`, `Math.random()` and similar calls give a different answer on every render, and a different one again on the server.

## Bad

```js
// @flow
export component Stamp() {
  const now = Date.now();
  return <time>{now}</time>;
}
```

```diagnostics
app/example.js:3:15 Cannot call impure function during render. `Date.now` is an impure function. Calling an impure function can produce unstable results that update unpredictably when the component happens to re-render. (https://react.dev/reference/rules/components-and-hooks-must-be-pure#components-and-hooks-must-be-idempotent).
```

## Good

```js
// @flow
export component Stamp(now: number) {
  return <time>{now}</time>;
}
```
