Props, state, context and values returned from hooks belong to React and must not be mutated. A mutation changes a value that React and the compiler assume did not change, so the screen and the data disagree. Copy the value and change the copy.

## Bad

```js
// @flow
export component Name(user: { name: string }) {
  user.name = user.name.trim();
  return <p>{user.name}</p>;
}
```

```diagnostics
app/example.js:3:3 This value cannot be modified. Modifying component props or hook arguments is not allowed. Consider using a local variable instead.
```

## Bad

A context value is shared by every component that reads it.

```js
// @flow
import { useContext } from "react";
import { Settings } from "./settings.js";

export component Theme() {
  const settings = useContext(Settings);
  settings.theme = "dark";
  return <p>{settings.theme}</p>;
}
```

```diagnostics
app/example.js:7:3 This value cannot be modified. Modifying a value returned from 'useContext()' is not allowed.
```

## Good

```js
// @flow
export component Name(user: { name: string }) {
  const name = user.name.trim();
  return <p>{name}</p>;
}
```
