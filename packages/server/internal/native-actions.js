// @flow
import { currentContext } from "./context.js";

export type NativeActionAuthorization = {|
  readonly request: Request,
  readonly id: string,
  readonly credential: string,
|};

function nativeRequest(request: Request): boolean {
  return (
    request.method === "POST" &&
    request.headers.get("uf-native-action") === "bearer-v1" &&
    !request.headers.has("origin") &&
    !request.headers.has("cookie") &&
    !Array.from(request.headers.keys()).some((name) => name.toLowerCase().startsWith("sec-fetch-"))
  );
}

/** Application-owned token verification, bound to exactly one action request. */
export async function authorizeNativeAction(
  request: Request,
  verify: (token: string, actionId: string) => Promise<boolean>,
): Promise<boolean> {
  const context = currentContext();
  if (context == null) throw new Error("authorizeNativeAction must run inside request middleware");
  context.nativeAction = null;
  if (!nativeRequest(request)) return false;
  const id = request.headers.get("uf-action");
  const credential = request.headers.get("authorization");
  if (id == null || !/^[a-f0-9]{64}$/.test(id) || credential == null) return false;
  const bearer = /^Bearer ([A-Za-z0-9\-._~+/]+=*)$/i.exec(credential);
  if (bearer == null || bearer[1].length > 8192) return false;
  let allowed = false;
  try {
    allowed = (await verify(bearer[1], id)) === true;
  } catch {
    return false;
  }
  if (
    !allowed ||
    !nativeRequest(request) ||
    request.headers.get("uf-action") !== id ||
    request.headers.get("authorization") !== credential
  )
    return false;
  context.nativeAction = { request, id, credential };
  return true;
}

export function nativeActionAllowed(request: Request): boolean {
  const proof = currentContext()?.nativeAction;
  return (
    proof != null &&
    proof.request === request &&
    nativeRequest(request) &&
    proof.id === request.headers.get("uf-action") &&
    proof.credential === request.headers.get("authorization")
  );
}
