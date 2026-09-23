A getter or setter runs code when a property is read or written, where a reader expects a plain field. Flow also cannot track what the side effect changes. Use a method, so the call is visible.

## Bad

```js
// @flow
export class Temperature {
  celsius: number = 0;
  get fahrenheit(): number {
    return this.celsius * 1.8 + 32;
  }
}
```

```diagnostics
app/example.js:4:3 avoid getters and setters; they hide side effects behind property access
```

## Good

```js
// @flow
export class Temperature {
  celsius: number = 0;
  fahrenheit(): number {
    return this.celsius * 1.8 + 32;
  }
}
```
