The HTML parser moves elements it does not allow where they were written: a `div` inside a `p` closes the paragraph, an `a` inside an `a` closes the outer link. The server's HTML then no longer matches the tree React built, and hydration fails or patches the page.

## Bad

```js
// @flow
export component Note() {
  return (
    <p>
      <div>Saved.</div>
    </p>
  );
}
```

```diagnostics
app/example.js:5:7 `<div>` inside `<p>` is not markup a browser keeps: the parser closes the `<p>` before it. React renders one tree and the parser builds another, which is a hydration mismatch rather than a matter of taste
```

## Good

```js
// @flow
export component Note() {
  return (
    <div>
      <p>Saved.</p>
    </div>
  );
}
```
