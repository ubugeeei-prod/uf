"use server";
// @flow

export async function failAction(): Promise<string> {
  throw new Error("action failure");
}
