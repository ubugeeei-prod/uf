Reading a named export off a default import, as in `React.useState` where `React` is the default export, only works when the default export happens to be an object that also carries that property. Import the named export itself.

## Bad

```js
// @flow
import React from "react";

export hook useCount(): number {
  const [count] = React.useState(0);
  return count;
}
```

## Good

```js
// @flow
import { useState } from "react";

export hook useCount(): number {
  const [count] = useState(0);
  return count;
}
```
