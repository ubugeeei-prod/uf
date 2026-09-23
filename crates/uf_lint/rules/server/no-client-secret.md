Everything a `"use client"` module reads ships to the browser. A server secret read there, such as a private token from `process.env`, ends up in the bundle anyone can download. Read secrets on the server and send the client only what it needs.

## Bad

```js
// @flow
"use client";

const token = process.env.PRIVATE_TOKEN;

export component Checkout() {
  return <form data-token={token} />;
}
```

```diagnostics
app/example.js:4:27 client modules must not read private server secrets
```

## Good

```js path=app/checkout.server.js
// @flow
export async function charge(amount: number): Promise<void> {
  const token = process.env.PRIVATE_TOKEN;
  await fetch("https://payments.example/charge", {
    body: JSON.stringify({ amount }),
    headers: { authorization: `Bearer ${token ?? ""}` },
    method: "POST",
  });
}
```
