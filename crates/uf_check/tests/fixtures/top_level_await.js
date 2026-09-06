// @flow

// A module that awaits its configuration before it exports anything: the
// shape ubugeeei-prod/uf#204 was filed about, and ES2022 since 2021.
//
// The type of `await p` is what `p` resolves to, so `settings` is the object
// and not the promise, and the annotation is the assertion.
const settings: { retries: number } = await Promise.resolve({ retries: 3 });

async function* names(): AsyncGenerator<string, void, void> {
  yield "retries";
}

const seen: Array<string> = [];
for await (const name of names()) {
  seen.push(name);
}

export const retries: number = settings.retries;
export const count: number = seen.length;
