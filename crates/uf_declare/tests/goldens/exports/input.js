// @flow
import type { Helper } from "./helper.js";
import { run } from "./helper.js";

export type { Helper };

export type Alias = Helper;

export { run };

export { run as go } from "./helper.js";

export * from "./helper.js";

export default function main(input: string): number {
  return input.length;
}
