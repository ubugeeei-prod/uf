A library definition that redeclares something Flow's built-in libraries already define, such as `Array` or `Promise`, silently replaces the real type for the whole project.

## Bad

```js
// @flow
// flow-typed/overrides.js
declare class Promise<+R> {
  then(onFulfill: (value: R) => mixed): Promise<mixed>;
}
```

## Good

```js
// @flow
// flow-typed/my-sdk.js
declare module "my-sdk" {
  declare export function connect(url: string): Promise<void>;
}
```
