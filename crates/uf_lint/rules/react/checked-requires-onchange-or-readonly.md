A `checked` input with no `onChange` is controlled by React and cannot be changed by anyone: every click is undone on the next render. Handle the change, mark it `readOnly`, or use `defaultChecked` for an uncontrolled box.

## Bad

```js
// @flow
export component Agree(accepted: boolean) {
  return <input type="checkbox" checked={accepted} />;
}
```

```diagnostics
app/example.js:3:33 `checked` makes this input controlled, so React puts the prop's value back after every click and the reader cannot move it; add an `onChange` that stores the new value, or `readOnly` to say that not moving is what was meant. `defaultChecked` is the uncontrolled form, which a click does move
```

## Good

```js
// @flow
export component Agree(accepted: boolean, onToggle: (next: boolean) => void) {
  return (
    <input
      type="checkbox"
      checked={accepted}
      onChange={(event) => onToggle(event.currentTarget.checked)}
    />
  );
}
```

## Good

```js
// @flow
export component Remembered() {
  return <input type="checkbox" defaultChecked />;
}
```
