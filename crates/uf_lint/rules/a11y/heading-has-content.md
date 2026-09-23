An empty heading still shows up in a screen reader's list of headings, as a heading with nothing in it. Readers navigate by those headings (WCAG 2.4.6).

## Bad

```js
// @flow
export component Section() {
  return (
    <section>
      <h2 />
      <p>Nothing here yet.</p>
    </section>
  );
}
```

```diagnostics
app/example.js:5:7 this `<h2>` has nothing a screen reader can announce, so a reader moving through the page by its headings lands on it and hears nothing; put the heading's words in it, or remove it
```

## Good

```js
// @flow
export component Section() {
  return (
    <section>
      <h2>Drafts</h2>
      <p>Nothing here yet.</p>
    </section>
  );
}
```
