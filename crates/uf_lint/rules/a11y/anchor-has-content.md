A link with no text and no label is announced as "link" and nothing more, so nobody using a screen reader can tell where it goes (WCAG 2.4.4, 4.1.2).

## Bad

```js
// @flow
export component Home() {
  return (
    <a href="/" />
  );
}
```

```diagnostics
app/example.js:4:5 this `<a>` is empty, so a screen reader reads it as "link" and nothing more, and WAI-ARIA requires a link to have a name; put the words in it, or name it with `aria-label` when it holds only an icon
```

## Good

```js
// @flow
export component Home() {
  return (
    <a href="/">Home</a>
  );
}
```
