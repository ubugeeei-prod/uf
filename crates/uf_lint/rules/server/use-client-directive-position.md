`"use client"` and `"use server"` only mark a module when they are its first statement. Written after an import, they are ordinary string expressions: the module silently stays on the other side of the boundary. A `"use server"` inside a function body marks that function as a server action and is fine.

## Bad

```js
// @flow
import { useState } from "react";
"use client";

export component Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(count + 1)} type="button">{count}</button>;
}
```

```diagnostics
app/example.js:3:1 a boundary directive is only honoured as the module's first statement
```

## Good

```js
// @flow
"use client";

import { useState } from "react";

export component Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(count + 1)} type="button">{count}</button>;
}
```
