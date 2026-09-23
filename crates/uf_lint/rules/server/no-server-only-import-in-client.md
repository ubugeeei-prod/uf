A client module is bundled for the browser along with everything it imports. Importing `@uniflowed/server` or a `*.server.js` module from one either fails the build or ships server code (database clients, file-system access) to the browser. Keep server work in server modules and pass results in as props or through server actions.

## Bad

```js
// @flow
"use client";

import { db } from "@uniflowed/server";
import { load } from "./orders.server.js";
```

```diagnostics
app/example.js:4:21 client modules must not import server-only modules; move the call behind a server action
app/example.js:5:31 client modules must not import server-only modules; move the call behind a server action
```

## Good

```js
// @flow
"use client";

import { formatPrice } from "./format.js";

export component Price(cents: number) {
  return <span>{formatPrice(cents)}</span>;
}
```
