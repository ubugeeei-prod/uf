A `label` that points at no control gives nothing its name: the field is announced without one, and clicking the label does not focus it (WCAG 1.3.1, 4.1.2). Point it at the control with `htmlFor`, or put the control inside it.

## Bad

```js
// @flow
export component Name() {
  return (
    <div>
      <label>Name</label>
      <input id="name" type="text" />
    </div>
  );
}
```

```diagnostics
app/example.js:5:7 this `<label>` is attached to no control, so clicking it does nothing and the field it describes has no accessible name; point `htmlFor` at the control's `id`, or put the control inside the label
```

## Good

```js
// @flow
export component Name() {
  return (
    <div>
      <label htmlFor="name">Name</label>
      <input id="name" type="text" />
    </div>
  );
}
```
