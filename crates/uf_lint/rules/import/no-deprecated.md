An export whose doc comment says `@deprecated` is on its way out, and the comment usually names its replacement. Importing it adds one more caller that has to move when it goes.

## Bad

```js path=app/api.js
// @flow
/** @deprecated Use `fetchUser`, which returns a typed result. */
export function getUser(id: string): Promise<mixed> {
  return fetch(`/users/${id}`).then((response) => response.json());
}

export function fetchUser(id: string): Promise<{ readonly name: string }> {
  return fetch(`/users/${id}`).then((response) => response.json());
}
```

```js path=app/page.js
// @flow
import { getUser } from "./api.js";

export const load = (id: string): Promise<mixed> => getUser(id);
```

```diagnostics
app/page.js:2:10 `getUser` is deprecated by `app/api.js`; use a supported export instead
```

## Good

```js path=app/api.js
// @flow
/** @deprecated Use `fetchUser`, which returns a typed result. */
export function getUser(id: string): Promise<mixed> {
  return fetch(`/users/${id}`).then((response) => response.json());
}

export function fetchUser(id: string): Promise<{ readonly name: string }> {
  return fetch(`/users/${id}`).then((response) => response.json());
}
```

```js path=app/page.js
// @flow
import { fetchUser } from "./api.js";

export const load = (id: string): Promise<{ readonly name: string }> => fetchUser(id);
```
