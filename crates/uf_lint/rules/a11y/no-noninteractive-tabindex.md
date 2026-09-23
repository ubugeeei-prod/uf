A `tabIndex` on content that does nothing adds a stop to the tab order that leads nowhere, and keyboard users have to walk through it on every pass.

## Bad

```js
// @flow
export component Notice() {
  return (
    <p tabIndex={0}>Your changes were saved.</p>
  );
}
```

```diagnostics
app/example.js:4:8 `<p>` is not a control, so this `tabIndex` adds a stop with nothing to do at it; drop it, or use `tabIndex={-1}` if something focuses this element itself
```

## Good

If the text must be announced, make it a live region instead of a tab stop.

```js
// @flow
export component Notice() {
  return (
    <p role="status">Your changes were saved.</p>
  );
}
```
