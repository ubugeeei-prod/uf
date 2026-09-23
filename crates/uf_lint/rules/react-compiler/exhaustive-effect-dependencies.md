An effect's dependency array says when it runs again. A value read inside the effect and missing from the array leaves the effect using a stale value; a value listed but never read re-runs it for nothing. Off by default, as in `eslint-plugin-react-hooks`.

## Bad

```js
// @flow
import { useEffect } from "react";

export component Title(count: number) {
  useEffect(() => {
    document.title = `${count} unread`;
  }, []);
  return null;
}
```

```diagnostics
app/example.js:6:25 Found missing effect dependencies. Missing dependencies can cause an effect to fire less often than it should.
```

## Good

```js
// @flow
import { useEffect } from "react";

export component Title(count: number) {
  useEffect(() => {
    document.title = `${count} unread`;
  }, [count]);
  return null;
}
```
