The compiler keeps every `useMemo` and `useCallback` it finds, so code that relies on a memoized value keeping its identity still works. When a dependency is changed after the memo reads it, the compiler cannot promise the same identity and skips the component instead of memoizing it differently.

## Bad

```js
// @flow
import { useMemo } from "react";
import { track } from "./track.js";

export component Count(a: number) {
  const list = [a];
  const size = useMemo(() => list.length, [list]);
  track(list);
  return <p>{size}</p>;
}
```

## Good

```js
// @flow
import { useMemo } from "react";
import { track } from "./track.js";

export component Count(a: number) {
  const list = [a];
  track(list);
  const size = useMemo(() => list.length, [list]);
  return <p>{size}</p>;
}
```
