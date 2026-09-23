A `useMemo` or `useCallback` recomputes when one of its dependencies changes. A value it reads but does not list leaves it returning a stale result; a value listed but not read recomputes it for nothing.

## Bad

```js
// @flow
import { useMemo } from "react";

export component Total(price: number, quantity: number) {
  const total = useMemo(() => price * quantity, [price]);
  return <output>{total}</output>;
}
```

```diagnostics
app/example.js:5:39 Found missing memoization dependencies. Missing dependencies can cause a value to update less often than it should, resulting in stale UI.
```

## Good

```js
// @flow
import { useMemo } from "react";

export component Total(price: number, quantity: number) {
  const total = useMemo(() => price * quantity, [price, quantity]);
  return <output>{total}</output>;
}
```
