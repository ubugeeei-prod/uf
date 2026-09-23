A module no other module imports, or an export nothing imports, is code that ships and has to be maintained without doing anything. Off by default, because entry points (routes, config files, scripts) are imported by the toolchain rather than by other modules. Switch it on to sweep a codebase for dead code.

## Bad

`legacy.js` is imported by nothing, and `lib.js`'s `unused` export is never imported.

```js path=app.js
// @flow
import { used } from "./lib.js";

used();
```

```js path=lib.js
// @flow
export function used() {}
export function unused() {}
```

```js path=legacy.js
// @flow
export const oldPrice = 100;
```

```diagnostics
legacy.js:1:1 no relative import reaches this module
lib.js:2:1 unused export: `unused`
```

## Good

```js path=app.js
// @flow
import { used } from "./lib.js";

used();
```

```js path=lib.js
// @flow
export function used() {}
```
