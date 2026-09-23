Elements such as `meta`, `script`, `style` and `html` are never rendered as content, so ARIA has nothing to describe on them. A `role` or `aria-*` there does nothing and suggests it does.

## Bad

```js
// @flow
export component Head() {
  return (
    <meta charSet="utf-8" aria-hidden="true" />
  );
}
```

```diagnostics
app/example.js:4:27 `<meta>` is one of the elements ARIA reserves: it is never rendered, so it is not in the accessibility tree and `aria-hidden` on it is read by nothing (WCAG 4.1.2); put it on the element a reader actually reaches
```

## Good

```js
// @flow
export component Head() {
  return (
    <meta charSet="utf-8" />
  );
}
```
