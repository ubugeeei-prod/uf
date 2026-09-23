An `iframe` without `sandbox` gives the embedded page everything a top-level page can do: run scripts, submit forms, open pop-ups and navigate your page away. Add `sandbox` and grant back only what the embed needs; an empty `sandbox` grants nothing.

## Bad

```js
// @flow
export component Map() {
  return <iframe src="https://maps.example/embed" title="Store location" />;
}
```

```diagnostics
app/example.js:3:10 this `<iframe>` has no `sandbox`, so the document inside it runs with everything a document gets — its own scripts, forms and popups, and the whole of this page if it is served from this origin; add `sandbox` to take all of that away, then name back only what the frame needs, such as `sandbox="allow-scripts"`
```

## Good

```js
// @flow
export component Map() {
  return (
    <iframe sandbox="allow-scripts" src="https://maps.example/embed" title="Store location" />
  );
}
```
