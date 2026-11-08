`"use client"` and `"use server"` only mark a module from its directive prologue. A mode directive (`"use flow";`, `"use js";` or `"use strict";`) may precede them; an import or any other statement may not. Written after one, they are ordinary string expressions: the module silently stays on the other side of the boundary. A `"use server"` inside a function body marks that function as a server action and is fine.

## Bad

```js
"use flow";
import { useState } from "react";
"use client";

export component Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(count + 1)} type="button">{count}</button>;
}
```

```diagnostics
app/example.js:3:1 a boundary directive is only honoured before any statement other than a mode directive
```

## Good

```js
"use flow";
"use client";

import { useState } from "react";

export component Counter() {
  const [count, setCount] = useState(0);
  return <button onClick={() => setCount(count + 1)} type="button">{count}</button>;
}
```
