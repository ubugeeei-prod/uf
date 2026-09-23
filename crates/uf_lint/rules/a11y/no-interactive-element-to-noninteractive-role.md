A `button` or a link keeps its keyboard behaviour whatever `role` it is given. Relabel it as something inert, such as `presentation`, and it still takes focus and still acts, but a screen reader no longer announces it as a control.

## Bad

```js
// @flow
export component RemoveTag(onRemove: () => void) {
  return (
    <button type="button" role="presentation" onClick={onRemove}>
      ×
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 `<button>` is a control a keyboard reaches and `role="presentation"` is not, so it keeps the behaviour and loses the announcement: focus lands on something a screen reader calls presentation; drop the role, or use an element that is one
```

## Good

```js
// @flow
export component RemoveTag(onRemove: () => void) {
  return (
    <button type="button" aria-label="Remove tag" onClick={onRemove}>
      ×
    </button>
  );
}
```
