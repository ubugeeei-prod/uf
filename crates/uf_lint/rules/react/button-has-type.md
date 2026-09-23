A `button` with no `type` is a submit button. Inside a form, a button meant to open a menu or clear a field submits the form instead. Write `type="button"`, `type="submit"` or `type="reset"` so the choice is on the page.

## Bad

```js
// @flow
export component Toolbar(onClear: () => void) {
  return <button onClick={onClear}>Clear</button>;
}
```

```diagnostics
app/example.js:3:10 this `<button>` has no `type`, and HTML defaults one to `type="submit"`: inside a form it submits and navigates away, so whatever its `onClick` did is thrown away by the navigation. Write `type="button"` for a button that runs a handler, or `type="submit"` to say the submission is meant
```

## Good

```js
// @flow
export component Toolbar(onClear: () => void) {
  return (
    <button type="button" onClick={onClear}>
      Clear
    </button>
  );
}
```
