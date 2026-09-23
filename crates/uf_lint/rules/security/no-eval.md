`eval`, `new Function` and a string passed to `setTimeout` or `setInterval` all compile text into code at run time. Any outside input that reaches that text runs with the page's full authority, and Content Security Policy has to be weakened to allow it at all. Pass a function instead.

## Bad

```js
// @flow
export function schedule(name: string): void {
  setTimeout(`refresh("${name}")`, 1000);
  eval(`window.${name}()`);
}
```

```diagnostics
app/example.js:3:3 a string timer body is evaluated as code; pass a function instead
app/example.js:4:3 `eval` executes arbitrary code; parse the data instead
```

## Good

```js
// @flow
declare function refresh(name: string): void;

export function schedule(name: string): void {
  setTimeout(() => refresh(name), 1000);
}
```
