Rendering must not change anything outside the component. A module variable reassigned during render changes on every render, including renders React throws away, and differs between the server and the browser.

## Bad

```js
// @flow
let renders = 0;

export component Counter() {
  renders = renders + 1;
  return <p>Rendered {renders} times</p>;
}
```

```diagnostics
app/example.js:5:3 Cannot reassign variables declared outside of the component/hook. Variable `renders` is declared outside of the component/hook. Reassigning this value during render is a form of side effect, which can cause unpredictable behavior depending on when the component happens to re-render. If this variable is used in rendering, use useState instead. Otherwise, consider updating it in an effect. (https://react.dev/reference/rules/components-and-hooks-must-be-pure#side-effects-must-run-outside-of-render).
```

## Good

```js
// @flow
import { useRef, useEffect } from "react";

export component Counter() {
  const renders = useRef(0);
  useEffect(() => {
    renders.current += 1;
  });
  return <p>Counting renders in an effect</p>;
}
```
