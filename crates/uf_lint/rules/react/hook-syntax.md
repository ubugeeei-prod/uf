Flow's `hook` syntax makes a hook a hook to the type checker: Flow then enforces the Rules of Hooks on its callers and knows its result follows React's rules. A plain `function useX` is just a function whose name looks like a hook.

## Bad

```js
// @flow
import { useState } from "react";

function useToggle(initial: boolean): [boolean, () => void] {
  const [on, setOn] = useState(initial);
  return [on, () => setOn(!on)];
}

export { useToggle };
```

```diagnostics
app/example.js:4:1 prefer Flow `hook` syntax for React hooks
```

## Good

```js
// @flow
import { useState } from "react";

export hook useToggle(initial: boolean): [boolean, () => void] {
  const [on, setOn] = useState(initial);
  return [on, () => setOn(!on)];
}
```
