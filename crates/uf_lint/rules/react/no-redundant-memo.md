The React Compiler memoizes every component and hook it compiles. A hand-written `useMemo` or `useCallback` in one of them is a second copy of work the compiler already did, with a dependency array that someone now has to keep correct. The rule reports only memoization the compiler removed.

## Bad

```js
// @flow
import { useMemo } from "react";

export component Total(prices: $ReadOnlyArray<number>) {
  const total = useMemo(() => prices.reduce((sum, price) => sum + price, 0), [prices]);
  return <output>{total}</output>;
}
```

```diagnostics
app/example.js:5:17 the React Compiler memoizes this already; `useMemo` here is a second dependency array to keep correct
```

## Good

```js
// @flow
export component Total(prices: $ReadOnlyArray<number>) {
  const total = prices.reduce((sum, price) => sum + price, 0);
  return <output>{total}</output>;
}
```
