The Rules of Hooks: call hooks at the top level of a component or hook, in the same order on every render. React matches each call to its state by position, so a hook behind a condition hands one hook's state to another.

## Bad

```js
// @flow
import { useState } from "react";

export component Search(enabled: boolean) {
  if (!enabled) {
    return null;
  }
  const [query, setQuery] = useState("");
  return <input value={query} onChange={(event) => setQuery(event.currentTarget.value)} />;
}
```

```diagnostics
app/example.js:8:29 Hooks must always be called in a consistent order, and may not be called conditionally. See the Rules of Hooks (https://react.dev/warnings/invalid-hook-call-warning)
```

## Good

```js
// @flow
import { useState } from "react";

export component Search(enabled: boolean) {
  const [query, setQuery] = useState("");
  if (!enabled) {
    return null;
  }
  return <input value={query} onChange={(event) => setQuery(event.currentTarget.value)} />;
}
```
