`dangerouslySetInnerHTML` writes a string into the page as HTML, so any user-controlled text that reaches it is a cross-site scripting hole. Render text as children, which React escapes, and render Markdown or rich text through `@uniflowed/markdown`, which sanitizes.

## Bad

```js
// @flow
export component Comment(body: string) {
  return <div dangerouslySetInnerHTML={{ __html: body }} />;
}
```

```diagnostics
app/example.js:3:15 unsanitized HTML is an XSS sink; render it through a @uniflowed/markdown helper
```

## Good

```js
// @flow
export component Comment(body: string) {
  return <p>{body}</p>;
}
```
