A `>` or `}` in JSX text is legal, but it is usually a tag or an expression typed one character wrong. Write the character as an entity, or as a string in braces, when you mean it.

## Bad

```js
// @flow
export component Breadcrumb() {
  return <p>Home > Settings</p>;
}
```

```diagnostics
app/example.js:3:18 `>` in JSX text renders as itself, and is usually what a mistyped tag or a dropped brace left behind; write `&gt;` if it is meant to be read
```

## Good

```js
// @flow
export component Breadcrumb() {
  return <p>Home &gt; Settings</p>;
}
```
