`autoFocus` moves focus as the page loads, before the user knows where they are. A screen reader starts reading from the middle of the page and skips everything above the field (WCAG 3.2.1).

## Bad

```js
// @flow
export component Search() {
  return (
    <input type="search" aria-label="Search" autoFocus />
  );
}
```

```diagnostics
app/example.js:4:46 `autoFocus` moves focus before the reader has been told where they are: a screen reader begins at this control instead of the top of the page, so the heading that says what the page is never gets read, and somebody using magnification is moved somewhere they did not ask to go; focus it from an event the reader caused instead
```

## Good

```js
// @flow
export component Search() {
  return (
    <input type="search" aria-label="Search" />
  );
}
```
