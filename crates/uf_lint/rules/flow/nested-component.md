A component declared inside another component or hook is a new component on every render, so React unmounts and remounts it each time and its state is lost. Declare components at the top level of a module.

## Bad

```js
// @flow
export component List(items: $ReadOnlyArray<string>) {
  component Row(label: string) {
    return <li>{label}</li>;
  }
  return <ul>{items.map((item) => <Row key={item} label={item} />)}</ul>;
}
```

```diagnostics
app/example.js:3:3 declare this component at module scope; nesting it remounts its subtree every render
```

## Good

```js
// @flow
component Row(label: string) {
  return <li>{label}</li>;
}

export component List(items: $ReadOnlyArray<string>) {
  return <ul>{items.map((item) => <Row key={item} label={item} />)}</ul>;
}
```
