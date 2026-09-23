An image with no `alt` is announced by its file name, or not at all, so a screen reader user loses whatever the image says (WCAG 1.1.1). Give `img`, `area` and `input type="image"` an `alt` with the words the image carries, or `alt=""` when it is decoration.

## Bad

```js
// @flow
export component Avatar() {
  return (
    <img src="/avatars/ada.png" />
  );
}
```

```diagnostics
app/example.js:4:5 `<img>` has no `alt`, so a screen reader announces its URL or nothing at all; give it the words the image is carrying, or `alt=""` when it carries none and the page already says them
```

## Good

```js
// @flow
export component Avatar() {
  return (
    <img src="/avatars/ada.png" alt="Ada Lovelace" />
  );
}
```
