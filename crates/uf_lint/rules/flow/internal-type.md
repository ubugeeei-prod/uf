Names like `React$Node` and `$TEMPORARY$object` are internal to Flow's library definitions and can change between Flow releases without notice. Import the public type instead.

## Bad

```js
// @flow
export type Slot = React$Node;
```

```diagnostics
app/example.js:2:20 this is a Flow-internal type; use the public equivalent
```

## Good

```js
// @flow
import type { Node } from "react";

export type Slot = Node;
```
