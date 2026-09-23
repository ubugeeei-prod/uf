Each `rel` keyword is defined for particular elements: `stylesheet` for `link`, `nofollow` for `a` and `area`. A keyword the element does not take is ignored, so the behaviour it was written for, such as `noopener`, silently does not happen.

## Bad

```js
// @flow
export component Styles() {
  return (
    <a href="/theme.css" rel="stylesheet">
      Theme
    </a>
  );
}
```

```diagnostics
app/example.js:4:26 `stylesheet` is a link type, but not one a `<a>` takes, so the browser ignores it and this element carries the relationship it was meant to declare nowhere. Drop it, or put it on the element the keyword belongs to
```

## Good

```js
// @flow
export component Styles() {
  return (
    <link href="/theme.css" rel="stylesheet" />
  );
}
```
