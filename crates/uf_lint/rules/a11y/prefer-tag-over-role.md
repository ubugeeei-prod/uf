An element that is the role, such as `button`, `nav` or `h2`, comes with the keyboard behaviour and the semantics the role only promises. A `div` with a `role` needs all of that written by hand.

## Bad

```js
// @flow
export component Links() {
  return (
    <div role="navigation">
      <a href="/docs">Docs</a>
    </div>
  );
}
```

```diagnostics
app/example.js:4:10 `role="navigation"` names what `<nav>` already is: the element carries the role without being told, keeps it when somebody moves or copies the markup, and brings the rest of its behaviour with it; write `<nav>` instead of a `<div>` with a role
```

## Good

```js
// @flow
export component Links() {
  return (
    <nav>
      <a href="/docs">Docs</a>
    </nav>
  );
}
```
