Reading an instance property in the constructor before it has been assigned reads `undefined`, whatever its declared type says.

## Bad

```js
// @flow
export class Cache {
  size: number;
  entries: Map<string, string>;

  constructor() {
    this.size = this.entries.size;
    this.entries = new Map();
  }
}
```

## Good

```js
// @flow
export class Cache {
  entries: Map<string, string>;
  size: number;

  constructor() {
    this.entries = new Map();
    this.size = this.entries.size;
  }
}
```
