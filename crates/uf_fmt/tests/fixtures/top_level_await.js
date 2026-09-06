// @flow
import { load, connect, stream } from "./io.js";

// Top-level `await` is ES2022 and it is a module's, not a function's, so the
// printer meets every await rule it already had at a nesting depth it never
// saw one at before. See ubugeeei-prod/uf#204.
const settings=await load()
await connect()

// The parenthesis rules are the interesting half: an await that is the object
// of a member or the callee of a call keeps its parentheses, and one that is
// neither does not.
const port = (await load()).port
const handler = (await connect())()
const merged = {...(await load()), port}
const listed = [...(await load()), port]

// Long enough to break, so what breaks is the group around the arguments and
// never the space after `await`.
const connection = await connect(settings.host, settings.port, settings.user, settings.password, settings.database)

for await (const row of stream()) { process.stdout.write(row) }

// Beside it, the shape that always parsed: `await` inside an `async` function
// is printed by the same code and must not have moved.
async function reload() {
  const next = await load()
  return (await connect(next)).id
}

// And the shape that is not an operator at all: `await` is an IdentifierName,
// so it is a property name in every goal symbol.
const names = { await: 1, async await() {} }
const read = (source) => source.await

export { settings, port, handler, merged, listed, connection, reload, names, read }
