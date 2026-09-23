A `useMemo` whose callback returns nothing memoizes `undefined`. It is usually an effect written in the wrong hook, or a missing `return`.

## Bad

```js
// @flow
import { useMemo } from "react";

export component Total(prices: $ReadOnlyArray<number>) {
  const total = useMemo(() => {
    prices.reduce((sum, price) => sum + price, 0);
  }, [prices]);
  return <output>{String(total)}</output>;
}
```

```diagnostics
app/example.js:5:25 useMemo() callbacks must return a value. This useMemo() callback doesn't return a value. useMemo() is for computing and caching values, not for arbitrary side effects.
```

## Good

```js
// @flow
import { useMemo } from "react";

export component Total(prices: $ReadOnlyArray<number>) {
  const total = useMemo(() => {
    return prices.reduce((sum, price) => sum + price, 0);
  }, [prices]);
  return <output>{total}</output>;
}
```
