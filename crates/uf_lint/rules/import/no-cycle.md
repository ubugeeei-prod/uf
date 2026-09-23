Two modules that import each other, directly or through others, are evaluated in an order that depends on which one is imported first. Whichever runs second finds the first one's exports still uninitialized. Break the cycle by moving what both need into a third module.

## Bad

`cart.js` imports `pricing.js`, which imports `cart.js` back.

```js path=app/cart.js
// @flow
import { priceOf } from "./pricing.js";

export type Line = { readonly sku: string, readonly quantity: number };
export const total = (lines: $ReadOnlyArray<Line>): number =>
  lines.reduce((sum, line) => sum + priceOf(line), 0);
```

```js path=app/pricing.js
// @flow
import type { Line } from "./cart.js";
import { total } from "./cart.js";

export const priceOf = (line: Line): number => line.quantity * 100;
export const average = (lines: $ReadOnlyArray<Line>): number => total(lines) / lines.length;
```

```diagnostics
app/cart.js:2:25 this import creates a cycle through `app/pricing.js`
app/pricing.js:2:27 this import creates a cycle through `app/cart.js`
```

## Good

The shared type moves to its own module, and `pricing.js` no longer reaches back into `cart.js`.

```js path=app/cart.js
// @flow
import type { Line } from "./line.js";
import { priceOf } from "./pricing.js";

export const total = (lines: $ReadOnlyArray<Line>): number =>
  lines.reduce((sum, line) => sum + priceOf(line), 0);
```

```js path=app/pricing.js
// @flow
import type { Line } from "./line.js";

export const priceOf = (line: Line): number => line.quantity * 100;
```

```js path=app/line.js
// @flow
export type Line = { readonly sku: string, readonly quantity: number };
```
