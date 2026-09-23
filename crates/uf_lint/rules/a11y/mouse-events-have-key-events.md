Something that happens on hover has to happen on focus too, or keyboard users never see it (WCAG 2.1.1). `onMouseOver` needs `onFocus` and `onMouseOut` needs `onBlur`.

## Bad

```js
// @flow
export component Hint(onShow: () => void, onHide: () => void) {
  return (
    <button type="button" onMouseOver={onShow} onMouseOut={onHide}>
      ?
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 `onMouseOver` fires for a pointer and nothing else, so whatever it does never happens for a keyboard; add `onFocus`, which is the same moment for somebody tabbing through
app/example.js:4:48 `onMouseOut` fires for a pointer and nothing else, so whatever it does never happens for a keyboard; add `onBlur`, which is the same moment for somebody tabbing through
```

## Good

```js
// @flow
export component Hint(onShow: () => void, onHide: () => void) {
  return (
    <button
      type="button"
      onMouseOver={onShow}
      onMouseOut={onHide}
      onFocus={onShow}
      onBlur={onHide}
    >
      ?
    </button>
  );
}
```
