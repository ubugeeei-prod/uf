Setting state synchronously in an effect renders the component a second time straight after the first. Effects are for synchronizing with things outside React; derive the value during render, or set it from the event that caused it.

## Bad

```js
// @flow
import { useEffect, useState } from "react";

export component Filtered(items: $ReadOnlyArray<string>, query: string) {
  const [visible, setVisible] = useState<$ReadOnlyArray<string>>([]);
  useEffect(() => {
    setVisible(items.filter((item) => item.includes(query)));
  }, [items, query]);
  return <p>{visible.length} matches</p>;
}
```

```diagnostics
app/example.js:7:5 Calling setState synchronously within an effect can trigger cascading renders. Effects are intended to synchronize state between React and external systems such as manually updating the DOM, state management libraries, or other platform APIs. In general, the body of an effect should do one or both of the following: * Update external systems with the latest state from React. * Subscribe for updates from some external system, calling setState in a callback function when external state changes. Calling setState synchronously within an effect body causes cascading renders that can hurt performance, and is not recommended. (https://react.dev/learn/you-might-not-need-an-effect).
```

## Good

```js
// @flow
export component Filtered(items: $ReadOnlyArray<string>, query: string) {
  const visible = items.filter((item) => item.includes(query));
  return <p>{visible.length} matches</p>;
}
```
