Importing a value from a module Flow has no types for gives you `any`, which switches off checking for everything that value flows into. Add types for the module, or import from one that has them.

## Bad

```js
// @flow
import leftPad from "left-pad"; // no types, no library definition

export const padded: string = leftPad("7", 3, "0");
```

## Good

```js
// @flow
export const padded: string = "7".padStart(3, "0");
```
