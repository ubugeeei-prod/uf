Some libraries return functions or objects whose results change without their identity changing, which memoization cannot see. The compiler knows which APIs these are and skips memoizing the code that uses them. This reports that it did, so a missing update is not a mystery.

## Bad

```js
// @flow
import { useVirtualizer } from "@tanstack/react-virtual";

export component Rows(count: number, parent: { current: HTMLElement | null }) {
  const virtualizer = useVirtualizer({
    count,
    getScrollElement: () => parent.current,
    estimateSize: () => 32,
  });
  return <div>{virtualizer.getVirtualItems().length}</div>;
}
```

```diagnostics
app/example.js:5:23 Use of incompatible library. This API returns functions which cannot be memoized without leading to stale UI. To prevent this, by default React Compiler will skip memoizing this component/hook. However, you may see issues if values from this API are passed to other components/hooks that are memoized.
```

## Good

```js
// @flow
export component Rows(labels: $ReadOnlyArray<string>) {
  return labels.map((label) => <div key={label}>{label}</div>);
}
```
