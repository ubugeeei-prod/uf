A link that opens with `target="_blank"` sends the current page's URL along as the referrer unless `rel` stops it, and older browsers also hand the new page a `window.opener` handle back to yours. Add `rel="noreferrer"`, which implies `noopener`.

## Bad

```js
// @flow
export component Docs() {
  return (
    <a href="https://docs.example.com" target="_blank">
      Documentation
    </a>
  );
}
```

```diagnostics
app/example.js:4:40 `<a target="_blank">` sends the `Referer` header on, so wherever this goes is told the full URL the reader is coming from — which in an application is a path with an order number, a document id or a search in it; add `rel="noreferrer"`, which stops it and covers `noopener` too (`noopener` on its own adds nothing, because `target="_blank"` already gives the opened page a null `window.opener`)
```

## Good

```js
// @flow
export component Docs() {
  return (
    <a href="https://docs.example.com" rel="noreferrer" target="_blank">
      Documentation
    </a>
  );
}
```
