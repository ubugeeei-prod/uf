"use server";
"use flow";

export async function failAction(): Promise<string> {
  throw new Error("action failure");
}
