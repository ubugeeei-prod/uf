// @flow
/** Configure an upstream service, never a URL supplied by the browser. */
export function endpoint(): string {
  return process.env.SNS_GRAPHQL_ENDPOINT ?? "http://127.0.0.1:4183";
}

/** Forward only the application's session, not unrelated cookies on the host. */
export function sessionCookie(value: string): string {
  return (
    value
      .split(";")
      .map((part) => part.trim())
      .find((part) => part.startsWith("commonplace_session=")) ?? ""
  );
}
