# Simple SNS

A small uf SSR app showing React 19.3, RSC, Server Actions, SQLite, StyleX,
Suspense, `use`, `useActionState`, `useFormStatus`, `useOptimistic`, Flow
`renders`, and Flow `match` in one readable social product surface.

Routes:

- `/` timeline with a server-loaded feed and optimistic posting
- `/messages` direct messages with a local SQLite backend
- `/settings` profile and notification settings
- `/login` demo login
- `/signup` demo signup

```sh
uf dev --cwd examples/simple-sns
uf build --cwd examples/simple-sns
uf test tests/library/examples.test.js
```
