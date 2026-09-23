State computed from props or other state in an effect renders twice: once with the stale value, then again with the new one. Compute the value during render instead. Off by default, as in `eslint-plugin-react-hooks`.

## Bad

```js
// @flow
import { useEffect, useState } from "react";

export component Name(first: string, last: string) {
  const [full, setFull] = useState("");
  useEffect(() => {
    setFull(`${first} ${last}`);
  }, [first, last]);
  return <p>{full}</p>;
}
```

```diagnostics
app/example.js:7:5 Values derived from props and state should be calculated during render, not in an effect. (https://react.dev/learn/you-might-not-need-an-effect#updating-state-based-on-props-or-state)
```

## Good

```js
// @flow
export component Name(first: string, last: string) {
  const full = `${first} ${last}`;
  return <p>{full}</p>;
}
```
