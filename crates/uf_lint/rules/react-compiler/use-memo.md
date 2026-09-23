`useMemo` calls its callback with no arguments and keeps what it returns. An `async` callback memoizes a promise, a generator memoizes an iterator, and a parameter is always `undefined`.

## Bad

```js
// @flow
import { useMemo } from "react";

export component Profile(id: string) {
  const user = useMemo(async () => {
    const response = await fetch(`/users/${id}`);
    return response.json();
  }, [id]);
  return <pre>{String(user)}</pre>;
}
```

```diagnostics
app/example.js:5:24 useMemo() callbacks may not be async or generator functions. useMemo() callbacks are called once and must synchronously return a value.
```

## Good

```js
// @flow
import { use, useMemo } from "react";

export component Profile(id: string) {
  const user = useMemo(() => fetch(`/users/${id}`).then((response) => response.json()), [id]);
  return <pre>{String(use(user))}</pre>;
}
```
