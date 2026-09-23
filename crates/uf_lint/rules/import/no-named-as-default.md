When a module exports `Button` by name and something else as its default, `import Button from "./Button.js"` reads as if it imports the named `Button` but actually gets the default export. Name the default import after what it is, or import the named export in braces.

## Bad

```js path=app/Button.js
// @flow
export component Button(label: string) {
  return <button type="button">{label}</button>;
}

export default component IconButton(icon: string) {
  return <button aria-label={icon} type="button" />;
}
```

```js path=app/page.js
// @flow
import Button from "./Button.js";
```

```diagnostics
app/page.js:2:8 `Button` is a named export of `app/Button.js`; import it by name instead of as the default
```

## Good

```js path=app/Button.js
// @flow
export component Button(label: string) {
  return <button type="button">{label}</button>;
}

export default component IconButton(icon: string) {
  return <button aria-label={icon} type="button" />;
}
```

```js path=app/page.js
// @flow
import IconButton, { Button } from "./Button.js";
```
