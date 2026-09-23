Setting state unconditionally during render starts another render, which sets it again: an infinite loop that React stops with an error.

## Bad

```js
// @flow
import { useState } from "react";

export component Greeting(name: string) {
  const [text, setText] = useState("");
  setText(`Hello, ${name}`);
  return <p>{text}</p>;
}
```

```diagnostics
app/example.js:6:3 Cannot call setState during render. Calling setState during render may trigger an infinite loop. * To reset state when other state/props change, store the previous value in state and update conditionally: https://react.dev/reference/react/useState#storing-information-from-previous-renders * To derive data from other state/props, compute the derived data during render without using state.
```

## Good

```js
// @flow
export component Greeting(name: string) {
  const text = `Hello, ${name}`;
  return <p>{text}</p>;
}
```
