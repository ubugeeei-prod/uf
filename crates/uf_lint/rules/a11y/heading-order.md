Headings are the outline a screen reader user navigates by. Jumping from `h2` to `h4` suggests a missing section, and readers go looking for it (WCAG 1.3.1).

## Bad

```js
// @flow
export component Article() {
  return (
    <article>
      <h2>Setup</h2>
      <h4>Install</h4>
    </article>
  );
}
```

```diagnostics
app/example.js:6:7 heading level jumps from `<h2>` to `<h4>`, and a reader navigating by heading reads the gap as a section that is missing; use `<h3>` and give it the size you wanted with CSS
```

## Good

```js
// @flow
export component Article() {
  return (
    <article>
      <h2>Setup</h2>
      <h3>Install</h3>
    </article>
  );
}
```
