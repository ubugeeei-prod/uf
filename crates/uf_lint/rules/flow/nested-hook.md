A hook declared inside a component or another hook is a new function on every render, and the Rules of Hooks cannot be checked across it. Declare hooks at the top level of a module.

## Bad

```js
// @flow
import { useState } from "react";

export component Counter() {
  hook useCount(): number {
    const [count] = useState(0);
    return count;
  }
  return <output>{useCount()}</output>;
}
```

```diagnostics
app/example.js:5:3 declare this hook at module scope; a nested hook gets a new identity every render
```

## Good

```js
// @flow
import { useState } from "react";

hook useCount(): number {
  const [count] = useState(0);
  return count;
}

export component Counter() {
  return <output>{useCount()}</output>;
}
```
