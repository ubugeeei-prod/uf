Each `aria-*` attribute takes a fixed kind of value: `true` or `false`, a token from a list, an id. A value outside that set is ignored, so the state a screen reader should announce is lost.

## Bad

```js
// @flow
export component Nav() {
  return (
    <a href="/docs" aria-current="yes">
      Docs
    </a>
  );
}
```

```diagnostics
app/example.js:4:21 `aria-current` takes one of `page`, `step`, `location`, `date`, `time`, `true`, `false`, and `yes` is not one, so the attribute is dropped and the state it was written for is never announced (WCAG 4.1.2)
```

## Good

```js
// @flow
export component Nav() {
  return (
    <a href="/docs" aria-current="page">
      Docs
    </a>
  );
}
```
