// @flow
export default async function Page() {
  await Promise.resolve();
  throw new Error("render failure");
}
