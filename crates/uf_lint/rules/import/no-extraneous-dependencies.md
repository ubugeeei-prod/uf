A package imported by name but not declared in the nearest `package.json` works only while something else happens to install it: a workspace neighbour, a hoisted transitive dependency. It breaks on the next clean install or in the published package. Declare every package you import.

## Bad

```json path=package.json
{ "name": "shop", "dependencies": { "react": "^19.0.0" } }
```

```js path=app/page.js
// @flow
import { useState } from "react";
import { format } from "date-fns";
```

```diagnostics
app/page.js:3:24 `date-fns` must be declared in the nearest package.json
```

## Good

```json path=package.json
{ "name": "shop", "dependencies": { "date-fns": "^4.0.0", "react": "^19.0.0" } }
```

```js path=app/page.js
// @flow
import { useState } from "react";
import { format } from "date-fns";
import { readFile } from "node:fs/promises";
import { total } from "./cart.js";
```
