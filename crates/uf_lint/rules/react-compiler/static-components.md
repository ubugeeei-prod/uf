A component created during render is a new component on every render, so React unmounts the old one and mounts the new one, throwing away its state and its DOM. Declare components at the top level of the module, and choose between existing ones instead of building one.

## Bad

```js
// @flow
export component Card(title: string) {
  const Heading = () => <h2>{title}</h2>;
  return (
    <section>
      <Heading />
    </section>
  );
}
```

```diagnostics
app/example.js:6:8 Cannot create components during render. Components created during render will reset their state each time they are created. Declare components outside of render.
```

## Bad

```js
// @flow
import { makeField } from "./fields.js";

export component Field(kind: string) {
  const Input = makeField(kind);
  return <Input />;
}
```

```diagnostics
app/example.js:6:11 Cannot create components during render. Components created during render will reset their state each time they are created. Declare components outside of render.
```

## Good

```js
// @flow
component Heading(title: string) {
  return <h2>{title}</h2>;
}

export component Card(title: string) {
  return (
    <section>
      <Heading title={title} />
    </section>
  );
}
```
