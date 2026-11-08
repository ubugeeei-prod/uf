Testing a Flow enum value for truthiness hides which members you meant, and a member added later passes silently. Compare against the members explicitly.

## Bad

```js
"use flow";
enum Status {
  Active,
  Paused,
}

export function isOn(status: ?Status): boolean {
  return status ? true : false;
}
```

## Good

```js
"use flow";
enum Status {
  Active,
  Paused,
}

export function isOn(status: ?Status): boolean {
  return status === Status.Active;
}
```
