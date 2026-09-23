A `component` or `hook` is a function called by React, not a method, so `this` inside one is `undefined`. It usually means code was moved over from a class component and still reads `this.props`.

## Bad

```js
// @flow
export component Title(text: string) {
  return <h1>{this.props.text}</h1>;
}
```

```diagnostics
app/example.js:3:15 `this` names nothing inside a `component`: Flow calls one as a plain function, so `this` is `undefined` here and reading anything off it throws; take the value from a parameter, or from the scope around the declaration
```

## Good

```js
// @flow
export component Title(text: string) {
  return <h1>{text}</h1>;
}
```
