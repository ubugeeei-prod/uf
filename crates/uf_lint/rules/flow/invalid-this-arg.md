Calling a method through `.call`, `.apply` or `.bind` with a receiver of the wrong type runs the method with a `this` it was never written for.

## Bad

```js
// @flow
class Counter {
  count: number = 0;
  increment(): void {
    this.count += 1;
  }
}

const counter = new Counter();
counter.increment.call({ total: 0 });
```

## Good

```js
// @flow
class Counter {
  count: number = 0;
  increment(): void {
    this.count += 1;
  }
}

const counter = new Counter();
counter.increment.call(counter);
```
