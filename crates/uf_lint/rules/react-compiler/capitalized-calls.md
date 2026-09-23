A function whose name starts with a capital is read as a component. Called like a function, it runs inside the caller's render with the caller's hooks, and the compiler cannot tell a component call from a plain function call. Render components with JSX; give helpers lower-case names.

## Bad

```js
// @flow
import { FormatPrice } from "./format.js";

export component Price(amount: number) {
  const text = FormatPrice(amount);
  return <span>{text}</span>;
}
```

```diagnostics
app/example.js:5:16 Capitalized functions are reserved for components, which must be invoked with JSX. If this is a component, render it with JSX. Otherwise, ensure that it has no hook calls and rename it to begin with a lowercase letter. Alternatively, if you know for a fact that this function is not a component, you can allowlist it via the compiler config. FormatPrice may be a component.
```

## Good

```js
// @flow
import { formatPrice } from "./format.js";

export component Price(amount: number) {
  const text = formatPrice(amount);
  return <span>{text}</span>;
}
```
