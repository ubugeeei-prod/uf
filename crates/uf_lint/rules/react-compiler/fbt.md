Meta's `fbt` internationalization library has JSX with rules of its own, and the compiler checks the parts it has to understand to compile it. Off by default: most projects do not use `fbt`.

## Bad

An `fbt` tag must be the module-level import, not a local value that happens to be called `fbt`.

```js
// @flow
export component Greeting(fbt: (text: string) => string) {
  return <fbt desc="Greeting">Hello</fbt>;
}
```

## Good

```js
// @flow
import fbt from "fbt";

export component Greeting(name: string) {
  return (
    <fbt desc="Greeting">
      Hello, <fbt:param name="name">{name}</fbt:param>
    </fbt>
  );
}
```
