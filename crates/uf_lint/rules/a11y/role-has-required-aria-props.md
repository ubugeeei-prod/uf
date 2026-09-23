Some roles are announced by their state: a `checkbox` by whether it is checked, a `slider` by its value. Without the attribute that carries that state, the reader hears the role and nothing else.

## Bad

```js
// @flow
export component Remember(onToggle: () => void) {
  return (
    <span role="checkbox" tabIndex={0} onClick={onToggle} onKeyDown={onToggle}>
      Remember me
    </span>
  );
}
```

```diagnostics
app/example.js:4:11 `role="checkbox"` tells a screen reader this is a checkbox, and `aria-checked` is the state a checkbox is announced by: without it the control is read out with nothing to say whether it is on, off or anywhere in between (WCAG 4.1.2); add it, or drop the role and use the HTML element that carries the state itself
```

## Good

```js
// @flow
export component Remember(checked: boolean, onToggle: () => void) {
  return (
    <span
      role="checkbox"
      aria-checked={checked}
      tabIndex={0}
      onClick={onToggle}
      onKeyDown={onToggle}
    >
      Remember me
    </span>
  );
}
```
