An `aria-*` attribute that ARIA does not define is ignored by every browser, so the state or name it was meant to carry never reaches assistive technology. It is usually a typo.

## Bad

```js
// @flow
export component Close() {
  return (
    <button type="button" aria-lable="Close">
      ×
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 `aria-lable` is not an ARIA attribute, so nothing reads it: the browser keeps it, no assistive technology looks at it, and the element stays unlabelled with no symptom to notice; did you mean `aria-label`?
```

## Good

```js
// @flow
export component Close() {
  return (
    <button type="button" aria-label="Close">
      ×
    </button>
  );
}
```
