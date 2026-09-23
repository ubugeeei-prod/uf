Between JSX tags, `//` and `/*` are text, not comments, so they end up on the page. A comment in JSX goes inside braces: `{/* like this */}`.

## Bad

```js
// @flow
export component Empty() {
  return <div>// nothing yet</div>;
}
```

```diagnostics
app/example.js:3:15 `//` between JSX tags is text, not a comment, so it shows on the page; write `{/* … */}` to comment it out, or `{"// …"}` if the slashes are meant to be seen
```

## Good

```js
// @flow
export component Empty() {
  return <div>{/* nothing yet */}</div>;
}
```
