The file must parse with the official Flow parser, the parser uf compiles with. When it does not, nothing else about the file can be checked, so this finding comes first.

## Bad

```js
// @flow
export const total = 1 +;
```

```diagnostics
app/example.js:2:25 Unexpected token `;`
```

## Good

```js
// @flow
export type Row = { id: string };
```
