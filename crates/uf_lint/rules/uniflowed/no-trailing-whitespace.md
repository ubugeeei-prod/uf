Whitespace at the end of a line is invisible in the editor and noisy in every diff that touches the line. `uf fmt` removes it. Spaces at the end of a line inside a template literal are part of the string and are left alone.

## Bad

```js
// @flow
export const greeting = "hello";   
```

```diagnostics
app/example.js:2:33 remove trailing whitespace
```

## Good

```js
// @flow
export const greeting = "hello";
```
