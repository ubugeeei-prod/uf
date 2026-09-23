A `@flow strict` module promises that everything it touches is checked strictly. Importing a module that is not strict breaks that promise at the import.

## Bad

```js
// @flow strict
import { format } from "./legacy-format.js"; // a `@flow` module, not strict

export const label: string = format(3);
```

## Good

```js
// @flow strict
import { format } from "./format.js"; // also `@flow strict`

export const label: string = format(3);
```
