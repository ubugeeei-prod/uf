A module that defines server actions with `serverAction` has to declare `"use server"` in its directive prologue. `"use flow";` may lead. Without the boundary, the bundler does not replace the actions with references, and the action bodies (and everything they import) are bundled into the client.

## Bad

```js path=app/actions.server.js
"use flow";
import { serverAction } from "@uniflowed/server";

export const save = serverAction(async (title: string) => {
  return { saved: title };
});
```

```diagnostics
app/actions.server.js:1:1 server action modules must declare "use server" before any statement other than a mode directive
```

## Good

```js path=app/actions.server.js
"use flow";
"use server";
import { serverAction } from "@uniflowed/server";

export const save = serverAction(async (title: string) => {
  return { saved: title };
});
```
