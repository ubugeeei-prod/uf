A relative path from one workspace package into another, such as `../../state/index.js` from `packages/web`, skips the other package's `exports` and its declared dependencies. It stops working as soon as either package is published or moved. Import the package by its name.

## Bad

```js path=packages/web/src/page.js
// @flow
import { createStore } from "../../state/index.js";
```

```diagnostics
packages/web/src/page.js:2:29 use the `state` package name instead of a relative path into it
```

## Good

```js path=packages/web/src/page.js
// @flow
import { createStore } from "@uniflowed/state";
import { Header } from "./header.js";
```
