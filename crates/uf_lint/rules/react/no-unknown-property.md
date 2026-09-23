React spells some DOM attributes differently from HTML: `className`, `htmlFor`, `tabIndex`. React warns about the HTML spelling at run time and may drop it; this reports it while you write it.

## Bad

```js
// @flow
export component Field() {
  return (
    <label class="field" for="email">
      Email
    </label>
  );
}
```

```diagnostics
app/example.js:4:12 `class` is the HTML spelling of this attribute and React spells the prop `className`, so the name written on this `<label>` is not the one React binds and whatever it was meant to do does not happen — React reports it as an unknown DOM property in development. Write `className`
app/example.js:4:26 `for` is the HTML spelling of this attribute and React spells the prop `htmlFor`, so the name written on this `<label>` is not the one React binds and whatever it was meant to do does not happen — React reports it as an unknown DOM property in development. Write `htmlFor`
```

## Good

```js
// @flow
export component Field() {
  return (
    <label className="field" htmlFor="email">
      Email
    </label>
  );
}
```
