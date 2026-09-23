React's `style` takes an object of properties, not a CSS string. A string throws at render time.

## Bad

```js
// @flow
export component Spacer() {
  return <div style="height: 1rem" />;
}
```

```diagnostics
app/example.js:3:15 `style` on this `<div>` is a string, and React's `style` prop takes an object mapping property names to values — it throws on anything else rather than rendering it, so this is a crash rather than a matter of taste. Write `style={{ color: "red" }}`, with the property names camelCased
```

## Good

```js
// @flow
export component Spacer() {
  return <div style={{ height: "1rem" }} />;
}
```
