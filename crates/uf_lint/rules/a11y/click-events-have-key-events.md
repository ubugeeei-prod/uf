An element with a `role` and an `onClick` is announced as a control, so keyboard users will try Enter or Space on it. Without a key handler nothing happens (WCAG 2.1.1).

## Bad

```js
// @flow
export component Toggle(onToggle: () => void) {
  return (
    <div role="button" tabIndex={0} onClick={onToggle}>
      Toggle
    </div>
  );
}
```

```diagnostics
app/example.js:4:37 `<div role="button">` answers a click and no key press, so a keyboard can reach it and still not use it; add an `onKeyDown` that runs the same handler
```

## Good

A real `button` gets Enter, Space and focus from the browser.

```js
// @flow
export component Toggle(onToggle: () => void) {
  return (
    <button type="button" onClick={onToggle}>
      Toggle
    </button>
  );
}
```
