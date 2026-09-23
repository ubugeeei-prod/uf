`export { value as default }` makes a default export that is easy to miss when reading the module's exports. Write `export default` where the value is declared.

## Bad

```js
// @flow
const page = { title: "Home" };
export { page as default };
```

```diagnostics
app/example.js:3:15 renaming an export to `default` hides the real name; export it directly
```

## Good

```js
// @flow
const page = { title: "Home" };
export default page;
```
