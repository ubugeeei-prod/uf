An index key ties each item's state to its position. When the list is reordered, filtered or added to at the front, React hands one item's state, focus and input text to another. Key by something the item carries, such as its id.

## Bad

```js
// @flow
type Row = { id: string, name: string };

export component Rows(rows: $ReadOnlyArray<Row>) {
  return rows.map((row, index) => <input key={index} defaultValue={row.name} />);
}
```

```diagnostics
app/example.js:5:42 `key` is built from the list index `index`, so React ties each item's state to its position and hands it to a different item when the list is reordered, filtered or added to at the front; key it by something the item carries, such as its id
```

## Good

```js
// @flow
type Row = { id: string, name: string };

export component Rows(rows: $ReadOnlyArray<Row>) {
  return rows.map((row) => <input key={row.id} defaultValue={row.name} />);
}
```
