React matches list items between renders by `key`. Without one it matches them by position, so state and focus stay where an item was rather than with the item when the list changes.

## Bad

```js
// @flow
type Todo = { id: string, title: string };

export component Todos(todos: $ReadOnlyArray<Todo>) {
  return <ul>{todos.map((todo) => <li>{todo.title}</li>)}</ul>;
}
```

```diagnostics
app/example.js:5:35 `<li>` is returned from `map` with no `key`, so React matches the items by position and hands one item's state to another when the list is reordered or filtered; give it `key={…}` with the item's id
```

## Good

```js
// @flow
type Todo = { id: string, title: string };

export component Todos(todos: $ReadOnlyArray<Todo>) {
  return (
    <ul>
      {todos.map((todo) => (
        <li key={todo.id}>{todo.title}</li>
      ))}
    </ul>
  );
}
```
