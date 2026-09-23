`?.` on a value that can never be `null` or `undefined`, such as `this`, suggests to a reader that it can be. Use a plain `.`.

## Bad

```js
// @flow
export class Timer {
  elapsed: number = 0;
  read(): number {
    return this?.elapsed;
  }
}
```

```diagnostics
app/example.js:5:12 `this` is never nullish; drop the `?.`
```

## Good

```js
// @flow
export class Timer {
  elapsed: number = 0;
  read(): number {
    return this.elapsed;
  }
}
```
