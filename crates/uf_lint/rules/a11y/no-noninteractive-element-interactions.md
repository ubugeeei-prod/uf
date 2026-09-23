Handlers on an element announced as content, such as an `article` or a paragraph, are never announced as anything to act on: a screen reader user has no reason to press a key there. Put the behaviour on a control.

## Bad

```js
// @flow
export component Story(onOpen: () => void) {
  return (
    <article onClick={onOpen} onKeyDown={onOpen}>
      Quarterly report
    </article>
  );
}
```

```diagnostics
app/example.js:4:5 `<article>` has the role `article` and is not a control, so handlers on it are reachable but never announced as anything to act on; move them to a `<button>`, or give the element a role that says what it does
```

## Good

```js
// @flow
export component Story(onOpen: () => void) {
  return (
    <article>
      <button type="button" onClick={onOpen}>
        Quarterly report
      </button>
    </article>
  );
}
```
