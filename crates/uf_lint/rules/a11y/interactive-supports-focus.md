An element given a widget role, such as `button`, `checkbox` or `tab`, is announced as a control, but a `div` or `span` is not in the tab order. Keyboard users hear about a control they cannot reach (WCAG 2.1.1).

## Bad

```js
// @flow
export component Star(onStar: () => void) {
  return (
    <span role="button" onClick={onStar} onKeyDown={onStar}>
      ★
    </span>
  );
}
```

```diagnostics
app/example.js:4:11 `role="button"` is a widget and `<span>` cannot take focus, so a keyboard never reaches the handler on it; give it `tabIndex={0}`, or use the element that is already this role
```

## Good

```js
// @flow
export component Star(onStar: () => void) {
  return (
    <span role="button" tabIndex={0} onClick={onStar} onKeyDown={onStar}>
      ★
    </span>
  );
}
```
