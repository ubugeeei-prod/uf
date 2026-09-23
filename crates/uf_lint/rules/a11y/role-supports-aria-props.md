Each role takes a fixed set of `aria-*` states. One its role does not take, such as `aria-checked` on a `link`, is ignored, so whatever state it was meant to carry is never announced.

## Bad

```js
// @flow
export component Nav() {
  return (
    <a href="/inbox" aria-checked="true">
      Inbox
    </a>
  );
}
```

```diagnostics
app/example.js:4:22 a `<a>` is a `link`, and ARIA gives `link` no `aria-checked`, so the attribute sits in the DOM with nothing to read it and the state it describes is never announced (WCAG 4.1.2); remove it, or give the element the role this state belongs to
```

## Good

```js
// @flow
export component Nav() {
  return (
    <a href="/inbox" aria-current="page">
      Inbox
    </a>
  );
}
```
