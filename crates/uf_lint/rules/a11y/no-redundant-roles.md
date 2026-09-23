A `role` the element already has, such as `role="button"` on a `button`, changes nothing and makes the reader of the code wonder what it was for.

## Bad

```js
// @flow
export component Save() {
  return (
    <button type="submit" role="button">
      Save
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 a `<button>` is already a `button`, so `role="button"` tells the browser what it told the browser: ARIA's first rule is to use the element and not to repeat it, and the copy is one more thing to keep true when the markup changes; drop the attribute
```

## Good

```js
// @flow
export component Save() {
  return (
    <button type="submit">Save</button>
  );
}
```
