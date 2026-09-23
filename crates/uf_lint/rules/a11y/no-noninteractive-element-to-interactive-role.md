Elements such as `h2`, `ul` or `article` already mean something to a screen reader. Giving one a control role such as `button` tells the reader one thing while the markup is another, and the control still needs every behaviour a real one has. Put the control element inside it.

## Bad

```js
// @flow
export component SectionToggle(onToggle: () => void) {
  return (
    <h2 role="button" tabIndex={0} onClick={onToggle} onKeyDown={onToggle}>
      Shipping
    </h2>
  );
}
```

```diagnostics
app/example.js:4:9 `<h2>` already has the role `heading` and `role="button"` says it is a control, so a screen reader is told one thing and the markup does another; build the control out of an element that has no semantics of its own, or use the element whose own role is `button`
```

## Good

```js
// @flow
export component SectionToggle(open: boolean, onToggle: () => void) {
  return (
    <h2>
      <button type="button" aria-expanded={open} onClick={onToggle}>
        Shipping
      </button>
    </h2>
  );
}
```
