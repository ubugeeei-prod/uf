`this` in an exported standalone function depends on how each caller happens to call it, and it is `undefined` in a plain call from another module. Take the value as a parameter.

## Bad

```js
// @flow
export function fullName(): string {
  return `${this.first} ${this.last}`;
}
```

## Good

```js
// @flow
export function fullName(person: { readonly first: string, readonly last: string }): string {
  return `${person.first} ${person.last}`;
}
```
