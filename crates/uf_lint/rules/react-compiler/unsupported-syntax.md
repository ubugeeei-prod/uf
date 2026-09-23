Syntax the React Compiler will not compile, such as `eval` or `with`, whose effects on the surrounding variables it cannot see. The function is left uncompiled.

## Bad

```js
// @flow
export component Formula(source: string) {
  const value = eval(source);
  return <output>{String(value)}</output>;
}
```

```diagnostics
app/example.js:3:17 The 'eval' function is not supported. Eval is an anti-pattern in JavaScript, and the code executed cannot be evaluated by React Compiler.
```

## Good

```js
// @flow
import { evaluate } from "./formula.js";

export component Formula(source: string) {
  const value = evaluate(source);
  return <output>{String(value)}</output>;
}
```
