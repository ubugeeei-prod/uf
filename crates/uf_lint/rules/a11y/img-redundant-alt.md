A screen reader already says "image" before an image's `alt`. Alt text that starts "image of" or "photo of" makes it say so twice, and adds nothing.

## Bad

```js
// @flow
export component Team() {
  return (
    <img src="/team.jpg" alt="Photo of the team at the offsite" />
  );
}
```

```diagnostics
app/example.js:4:26 this `alt` says "photo", but a screen reader already announces an `<img>` as an image, so a reader hears it twice; describe what the image shows, such as `alt="Ada at the summit"` rather than `alt="Photo of Ada at the summit"`
```

## Good

```js
// @flow
export component Team() {
  return (
    <img src="/team.jpg" alt="The team at the offsite" />
  );
}
```
