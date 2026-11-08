`Object.assign` mutates its first argument, and Flow types the merge less precisely than an object spread. Spread into a new object.

## Bad

```js
"use flow";
export function withDefaults(options: { readonly size?: number }): { readonly size?: number } {
  return Object.assign({}, { size: 10 }, options);
}
```

```diagnostics
app/example.js:3:10 prefer object spread over `Object.assign`, which mutates its target
```

## Good

```js
"use flow";
export function withDefaults(options: { readonly size?: number }): { readonly size?: number } {
  return { size: 10, ...options };
}
```
