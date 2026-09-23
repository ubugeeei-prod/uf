React cannot render a namespaced element name such as `svg:rect`. JSX parses it, but React DOM has no element to create for it. Write the element's own name.

## Bad

```js
// @flow
export component Dot() {
  return (
    <svg viewBox="0 0 10 10">
      <svg:circle cx="5" cy="5" r="5" />
    </svg>
  );
}
```

```diagnostics
app/example.js:5:8 `<svg:circle>` is a namespaced element name, which React does not support: JSX reads a lowercase name as an HTML tag and a capitalised one as a value in scope, and `svg:circle` is neither, so there is nothing for React to render. Inside an `<svg>` the children are already in the SVG namespace — write `<circle>`
```

## Good

```js
// @flow
export component Dot() {
  return (
    <svg viewBox="0 0 10 10">
      <circle cx="5" cy="5" r="5" />
    </svg>
  );
}
```
