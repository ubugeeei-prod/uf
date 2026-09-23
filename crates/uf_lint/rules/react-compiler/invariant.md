An internal assumption of the React Compiler that did not hold: a bug in the compiler, not a rule your code broke. The function is left uncompiled and runs as written. Off by default; turn it on when you want to know which functions the compiler gave up on, or to report a compiler bug.

## Bad

A named function expression that refers to itself trips an assertion in this compiler version.

```js
// @flow
export component Countdown(from: number) {
  const tick = function step(n: number): $ReadOnlyArray<number> {
    return n <= 0 ? [] : [n, ...step(n - 1)];
  };
  return <p>{tick(from).join(", ")}</p>;
}
```

```diagnostics
app/example.js:4:33 [InferMutationAliasingEffects] Expected value kind to be initialized. <unknown> step$8:TFunction.
```

## Good

```js
// @flow
function step(n: number): $ReadOnlyArray<number> {
  return n <= 0 ? [] : [n, ...step(n - 1)];
}

export component Countdown(from: number) {
  return <p>{step(from).join(", ")}</p>;
}
```
