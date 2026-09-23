A namespace import (`import * as ns`) is a module object that Flow can only check when you read members off it. Passing the namespace around as a value loses those checks.

## Bad

```js
// @flow
import * as math from "./math.js";

export const helpers = math;
```

## Good

```js
// @flow
import * as math from "./math.js";

export const total: number = math.sum([1, 2, 3]);
```
