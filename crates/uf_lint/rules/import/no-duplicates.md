Two `import` statements from the same module can be written as one. Keeping them apart means the next reader has to look in two places to see what a module takes from another. A type import beside a value import is not a duplicate, since Flow keeps them apart.

## Bad

```js
// @flow
import { useState } from "react";
import { useEffect } from "react";
```

```diagnostics
app/example.js:3:27 `react` is already imported in this file
```

## Good

```js
// @flow
import { useEffect, useState } from "react";
import type { Node } from "react";
```
