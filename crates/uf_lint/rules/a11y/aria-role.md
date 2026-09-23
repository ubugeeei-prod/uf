A `role` that is not an ARIA role, or is one of the abstract roles ARIA uses only to organise its own taxonomy, is ignored, and the element is announced as whatever it was before.

## Bad

```js
// @flow
export component Toolbar() {
  return (
    <div role="toolbox">
      <button type="button">Bold</button>
    </div>
  );
}
```

```diagnostics
app/example.js:4:10 `toolbox` is not an ARIA role, so nothing reads it: the browser keeps the attribute, no assistive technology looks at it, and the element is announced as whatever HTML made it (WCAG 4.1.2); did you mean `toolbar`?
```

## Good

```js
// @flow
export component Toolbar() {
  return (
    <div role="toolbar">
      <button type="button">Bold</button>
    </div>
  );
}
```
