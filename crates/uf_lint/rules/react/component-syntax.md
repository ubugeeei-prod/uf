Flow's `component` syntax types props exactly, lets `renders` say what a component may render, and is what the React Compiler reads a component as. A function or arrow that returns JSX gets none of that.

## Bad

```js
// @flow
import * as React from "react";

export function Greeting({ name }: { name: string }): React.Node {
  return <p>Hello, {name}</p>;
}
```

```diagnostics
app/example.js:4:1 prefer Flow `component` syntax for React components
```

## Good

```js
// @flow
export component Greeting(name: string) {
  return <p>Hello, {name}</p>;
}
```

## Good

A component whose type says what it renders.

```js
// @flow
component Item(label: string) {
  return <li>{label}</li>;
}

export component List(labels: $ReadOnlyArray<string>) renders* Item {
  return labels.map((label) => <Item key={label} label={label} />);
}
```
