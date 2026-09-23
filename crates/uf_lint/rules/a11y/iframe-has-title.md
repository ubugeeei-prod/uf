A screen reader announces an `iframe` by its `title`. Without one, the reader hears "frame" and has to go in to find out what it holds (WCAG 4.1.2).

## Bad

```js
// @flow
export component Map() {
  return (
    <iframe src="https://maps.example.com/embed?q=office" />
  );
}
```

```diagnostics
app/example.js:4:5 this `<iframe>` has no `title`, so a screen reader announces a frame with no name and a reader has to go inside to learn what it holds (WCAG 4.1.2); give it a `title` that says, such as `title="Map of the venue"`
```

## Good

```js
// @flow
export component Map() {
  return (
    <iframe src="https://maps.example.com/embed?q=office" title="Map of our office" />
  );
}
```
