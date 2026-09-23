`aria-hidden` removes an element from what a screen reader announces, but not from the tab order. A focusable element hidden that way takes focus and is announced as nothing (WCAG 4.1.2).

## Bad

```js
// @flow
export component Close(onClose: () => void) {
  return (
    <button type="button" aria-hidden="true" onClick={onClose}>
      ×
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 `<button>` is hidden from assistive technology but still takes focus, so a keyboard lands on an element a screen reader cannot announce; drop the `aria-hidden`, or take it out of the tab order with `tabIndex={-1}`
```

## Good

Hide the decorative glyph, not the control.

```js
// @flow
export component Close(onClose: () => void) {
  return (
    <button type="button" aria-label="Close" onClick={onClose}>
      <span aria-hidden="true">×</span>
    </button>
  );
}
```
