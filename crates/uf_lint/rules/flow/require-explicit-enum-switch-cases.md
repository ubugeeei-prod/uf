A `default` branch in a `switch` over a Flow enum catches members added later without anyone deciding what they should do. List every member, so that adding one is a type error until it is handled. `match` gives the same exhaustiveness check.

## Bad

```js
// @flow
enum Status {
  Active,
  Paused,
  Closed,
}

export function label(status: Status): string {
  switch (status) {
    case Status.Active:
      return "active";
    default:
      return "inactive";
  }
}
```

## Good

```js
// @flow
enum Status {
  Active,
  Paused,
  Closed,
}

export function label(status: Status): string {
  return match (status) {
    Status.Active => "active",
    Status.Paused => "paused",
    Status.Closed => "closed",
  };
}
```
