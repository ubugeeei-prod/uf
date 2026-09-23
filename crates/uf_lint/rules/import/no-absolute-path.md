An import that starts at the filesystem root, such as `/Users/me/app/format.js` or `C:\work\app\format.js`, only resolves on the machine that wrote it. Import packages by name and project files by a relative path.

## Bad

```js
// @flow
import { format } from "/Users/me/app/lib/format.js";
```

```diagnostics
app/example.js:2:24 import paths must be portable; use a package name or a relative path
```

## Good

```js
// @flow
import { format } from "./lib/format.js";
import { useState } from "react";
```
