A `tabIndex` above zero moves the element ahead of everything else in the tab order, across the whole page. Keyboard users then jump around the page in an order that matches nothing they see (WCAG 2.4.3).

## Bad

```js
// @flow
export component Login() {
  return (
    <input type="email" aria-label="Email" tabIndex={1} />
  );
}
```

```diagnostics
app/example.js:4:44 a `tabIndex` of 1 puts this element ahead of everything the document orders itself, so the tab order stops matching the reading order; give it `tabIndex={0}` and let its position in the markup decide
```

## Good

```js
// @flow
export component Login() {
  return (
    <input type="email" aria-label="Email" />
  );
}
```
