A module that defines server actions with `serverAction` has to open with `"use server"`. Without it, the bundler does not replace the actions with references, and the action bodies (and everything they import) are bundled into the client.

## Bad

```js path=app/actions.server.js
// @flow
import { serverAction } from "@uniflowed/server";

export const save = serverAction(async (title: string) => {
  return { saved: title };
});
```

```diagnostics
app/actions.server.js:1:1 server action modules must start with "use server";
```

## Good

```js path=app/actions.server.js
"use server";
// @flow
import { serverAction } from "@uniflowed/server";

export const save = serverAction(async (title: string) => {
  return { saved: title };
});
```
