A module that imports itself gets its own half-initialized exports back, which is never what was meant. It is usually a leftover from moving code between files.

## Bad

```js path=app/format.js
// @flow
import { pad } from "./format.js";

export const pad = (value: number): string => String(value).padStart(2, "0");
export const clock = (hours: number, minutes: number): string => `${pad(hours)}:${pad(minutes)}`;
```

```diagnostics
app/format.js:2:21 a module must not import itself; move shared code to another module
```

## Good

```js path=app/format.js
// @flow
export const pad = (value: number): string => String(value).padStart(2, "0");
export const clock = (hours: number, minutes: number): string => `${pad(hours)}:${pad(minutes)}`;
```
