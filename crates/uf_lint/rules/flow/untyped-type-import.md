Importing a type from a module Flow has no types for gives you an `any` alias, which checks nothing wherever you use it. Add types for the module, or declare the type yourself.

## Bad

```js
// @flow
import type { Options } from "legacy-widget"; // no types, no library definition

export const defaults: Options = { size: 10 };
```

## Good

```js
// @flow
export type Options = { readonly size: number };

export const defaults: Options = { size: 10 };
```
