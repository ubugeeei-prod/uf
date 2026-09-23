A `div` or `span` with an `onClick` cannot be reached with the keyboard and is announced as nothing in particular, so the action it performs is available only to a mouse (WCAG 2.1.1).

## Bad

```js
// @flow
export component Card(onOpen: () => void) {
  return (
    <div onClick={onOpen}>Open the report</div>
  );
}
```

```diagnostics
app/example.js:4:5 `<div>` has an `onClick` but no `role` and no key handler, so nothing but a mouse can reach it; make it a `<button>`, or give it a `role` and an `onKeyDown`
```

## Good

```js
// @flow
export component Card(onOpen: () => void) {
  return (
    <button type="button" onClick={onOpen}>
      Open the report
    </button>
  );
}
```
