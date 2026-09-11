// @flow
export class InputError extends Error {
  fields: { [string]: string };
  constructor(message: string, fields: { [string]: string } = {}) {
    super(message);
    this.fields = fields;
  }
}
export function field(form: FormData, name: string, max: number, min: number = 0): string {
  if (!(form instanceof FormData)) throw new InputError("Please submit the form again.");
  const value = form.get(name);
  if (value != null && typeof value !== "string") throw new InputError("Files are not accepted.");
  const text = typeof value === "string" ? value.trim() : "";
  if (text.length < min || text.length > max)
    throw new InputError("Please check the highlighted field.", {
      [name]:
        min > 0 && text.length === 0 ? "This field is required." : `Use ${min}–${max} characters.`,
    });
  return text;
}
export function identifier(value: string): string {
  if (typeof value !== "string" || !/^[a-zA-Z0-9-]{1,100}$/.test(value))
    throw new InputError("That item is not available.");
  return value;
}
export function handleField(form: FormData): string {
  const value = field(form, "handle", 20, 3).toLowerCase();
  if (!/^[a-z][a-z0-9_]{2,19}$/.test(value))
    throw new InputError("Please check your handle.", {
      handle: "Start with a letter; use 3–20 letters, numbers, or underscores.",
    });
  return value;
}
export function emailField(form: FormData): string {
  const value = field(form, "email", 254, 1);
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value))
    throw new InputError("Please check your email.", { email: "Enter a valid email address." });
  return value;
}
