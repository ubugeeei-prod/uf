Browsers and password managers fill a field from its `autoComplete` token, and assistive tools use it to show a familiar icon (WCAG 1.3.5). A token the HTML standard does not define fills nothing.

## Bad

```js
// @flow
export component Email() {
  return (
    <input type="email" name="email" autoComplete="mail" />
  );
}
```

```diagnostics
app/example.js:4:38 `mail` is not an autofill token, so the browser cannot tell what this field is for and fills nothing — which WCAG 1.3.5 asks for, and which matters most to somebody for whom typing an address again is the hard part; use a token from the HTML autofill list, such as `name`, `email`, `street-address` or `cc-number`, or `off` on its own
```

## Good

```js
// @flow
export component Email() {
  return (
    <input type="email" name="email" autoComplete="email" />
  );
}
```
