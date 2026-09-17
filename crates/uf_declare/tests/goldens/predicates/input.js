// @flow

export function isNumber(value: mixed): value is number {
  return typeof value === "number";
}

export function isString(value: mixed): implies value is string {
  return typeof value === "string";
}
