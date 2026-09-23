`aria-activedescendant` tells assistive technology which option is current while focus stays on the container. That only works when the container can take focus, so an element that is not focusable by default needs a `tabIndex`.

## Bad

```js
// @flow
export component Options() {
  return (
    <div role="listbox" aria-activedescendant="option-1">
      <div id="option-1" role="option" aria-selected="true">
        One
      </div>
    </div>
  );
}
```

```diagnostics
app/example.js:4:25 `aria-activedescendant` says which element focus is standing in for, and `<div>` cannot take focus, so nothing ever reads it; give it `tabIndex={0}`
```

## Good

```js
// @flow
export component Options() {
  return (
    <div role="listbox" aria-activedescendant="option-1" tabIndex={0}>
      <div id="option-1" role="option" aria-selected="true">
        One
      </div>
    </div>
  );
}
```
