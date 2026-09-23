When a prop is written twice, the last one wins and the first is silently thrown away. It is almost always a copy-and-paste mistake.

## Bad

```js
// @flow
export component Avatar(src: string) {
  return <img src={src} alt="" alt="Profile picture" />;
}
```

```diagnostics
app/example.js:3:32 `alt` is given twice on `<img>`, and only the last one reaches it; remove the one that was not meant
```

## Good

```js
// @flow
export component Avatar(src: string) {
  return <img src={src} alt="Profile picture" />;
}
```
