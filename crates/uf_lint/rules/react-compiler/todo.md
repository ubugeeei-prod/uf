Code the React Compiler cannot compile yet. The function is left as it is, unmemoized, and the rest of the module is still compiled. Off by default: nothing is wrong with the code, and the finding says what the compiler skipped.

## Bad

```js
// @flow
export component Temperature(celsius: number) {
  const reading = {
    get fahrenheit(): number {
      return (celsius * 9) / 5 + 32;
    },
  };
  return <p>{reading.fahrenheit}°F</p>;
}
```

```diagnostics
app/example.js:4:5 (BuildHIR::lowerExpression) Handle get functions in ObjectExpression
```

## Good

```js
// @flow
export component Temperature(celsius: number) {
  const fahrenheit = (celsius * 9) / 5 + 32;
  return <p>{fahrenheit}°F</p>;
}
```
