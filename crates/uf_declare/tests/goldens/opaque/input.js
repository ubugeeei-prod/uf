// @flow

export opaque type Id = string;

export opaque type Token: string = string;

export function makeId(raw: string): Id {
  return raw;
}

export function readId(id: Id): string {
  return id;
}

export function useToken(token: Token): string {
  return token;
}
