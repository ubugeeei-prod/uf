A ref is a value React does not track. Reading or writing `ref.current` during render shows whatever it held last time and does not update the screen when it changes. Use refs in effects and event handlers, and state for what is rendered.

## Bad

```js
// @flow
import { useRef } from "react";

export component Clicks() {
  const count = useRef(0);
  return <button type="button" onClick={() => { count.current += 1; }}>{count.current}</button>;
}
```

```diagnostics
app/example.js:6:73 Cannot access refs during render. React refs are values that are not needed for rendering. Refs should only be accessed outside of render, such as in event handlers or effects. Accessing a ref value (the `current` property) during render can cause your component not to update as expected (https://react.dev/reference/react/useRef).
```

## Good

```js
// @flow
import { useState } from "react";

export component Clicks() {
  const [count, setCount] = useState(0);
  return (
    <button type="button" onClick={() => setCount(count + 1)}>
      {count}
    </button>
  );
}
```
