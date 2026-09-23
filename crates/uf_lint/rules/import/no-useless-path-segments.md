`./features/../model.js` is `./model.js` with a detour. The longer path makes a reader resolve it in their head and hides what the module really depends on. Write the shortest path that names the same file.

## Bad

```js path=app/page.js
// @flow
import { load } from "./features/../model.js";
```

```diagnostics
app/page.js:2:22 `./features/../model.js` can be written as `./model.js`
```

## Good

```js path=app/page.js
// @flow
import { load } from "./model.js";
import { routes } from "./routes/index.js";
```
