A button, input or other control with no text, no label and no `aria-label` is announced only by its role: "button". Nobody can tell what it does (WCAG 4.1.2).

## Bad

```js
// @flow
export component Search(onSearch: () => void) {
  return (
    <button type="button" onClick={onSearch} />
  );
}
```

```diagnostics
app/example.js:4:5 this `<button>` has no name and no `id` for a `<label>` to point at, so a screen reader announces only what kind of control it is; give it an `aria-label`, put the words inside it, or give it an `id` and point a `<label htmlFor>` at that
```

## Good

```js
// @flow
export component Search(onSearch: () => void) {
  return (
    <button type="button" aria-label="Search" onClick={onSearch} />
  );
}
```
