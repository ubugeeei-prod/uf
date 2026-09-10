// @flow
//
// `@uniflowed/graphql`: the Relay environment, without the boilerplate.
//
// The environment it builds is Relay's own, so there is no point testing what
// Relay does with it. What is worth testing is the twenty lines uf wrote: the
// request that goes out, and the fact that a GraphQL error arriving with a 200
// is an error rather than a `null` rendered silently.

import { describe, expect, it } from "@uniflowed/testing";
import { createEnvironment, GraphQlResponseError } from "@uniflowed/graphql";

/** A fetch client that records what it was asked for and answers `payload`. */
function recording(payload: mixed, status: number = 200) {
  const calls: Array<{ path: string, options: mixed }> = [];
  const client = {
    raw: async (path: string, options: mixed) => {
      calls.push({ path, options });
      return new Response(JSON.stringify(payload), {
        status,
        headers: { "content-type": "application/json" },
      });
    },
    request: async () => {
      throw new Error("the environment must not use request()");
    },
    extend: () => client,
  };
  return { client, calls };
}

/** The shape Relay hands its network function. */
const operation = { name: "ViewerQuery", text: "query ViewerQuery { viewer { id } }" };

/** Reach the network function the environment was built with. */
function networkOf(environment: mixed) {
  return (environment: $FlowFixMe).getNetwork();
}

describe("createEnvironment", () => {
  it("builds a Relay environment", () => {
    const { client } = recording({ data: {} });

    const environment = createEnvironment({
      endpoint: "/graphql",
      fetch: client,
    });

    expect(typeof (environment: $FlowFixMe).execute).toBe("function");
    expect(typeof (environment: $FlowFixMe).getStore).toBe("function");
  });

  it("gives each environment its own store", () => {
    // One environment per request is the only way two users on a server do not
    // see each other's data.
    const { client } = recording({ data: {} });
    const first = createEnvironment({ endpoint: "/graphql", fetch: client });
    const second = createEnvironment({ endpoint: "/graphql", fetch: client });

    expect((first: $FlowFixMe).getStore()).not.toBe((second: $FlowFixMe).getStore());
  });
});

describe("the request it sends", () => {
  it("posts the operation to the endpoint", async () => {
    const { client, calls } = recording({ data: { viewer: null } });
    const environment = createEnvironment({ endpoint: "/api/graphql", fetch: client });

    await networkOf(environment).execute(operation, { id: "1" }, {}).toPromise();

    expect(calls.length).toBe(1);
    expect(calls[0].path).toBe("/api/graphql");
    const options = (calls[0].options: $FlowFixMe);
    expect(options.method).toBe("POST");
    expect(options.body.query).toBe(operation.text);
    expect(options.body.variables).toEqual({ id: "1" });
  });

  it("sends the operation's name, so a server's logs are readable", async () => {
    const { client, calls } = recording({ data: {} });
    const environment = createEnvironment({ endpoint: "/graphql", fetch: client });

    await networkOf(environment).execute(operation, {}, {}).toPromise();

    expect((calls[0].options: $FlowFixMe).body.operationName).toBe("ViewerQuery");
  });

  it("sends the headers it was configured with", async () => {
    const { client, calls } = recording({ data: {} });
    const environment = createEnvironment({
      endpoint: "/graphql",
      fetch: client,
      headers: { authorization: "Bearer t" },
    });

    await networkOf(environment).execute(operation, {}, {}).toPromise();

    const headers = (calls[0].options: $FlowFixMe).headers;
    expect(headers.authorization).toBe("Bearer t");
    expect(headers.accept).toContain("application/json");
  });
});

describe("errors", () => {
  it("throws when the response carries errors, even with a 200", async () => {
    // The failure mode this exists to prevent: a client that only checks the
    // status renders `null` and says nothing about why.
    const { client } = recording({
      data: null,
      errors: [{ message: "not authorised" }],
    });
    const environment = createEnvironment({ endpoint: "/graphql", fetch: client });

    let thrown = null;
    try {
      await networkOf(environment).execute(operation, {}, {}).toPromise();
    } catch (error) {
      thrown = error;
    }

    expect(thrown instanceof GraphQlResponseError).toBe(true);
    expect(String(thrown)).toContain("not authorised");
  });

  it("keeps every error, and counts them in the message", async () => {
    const { client } = recording({
      errors: [{ message: "first" }, { message: "second" }],
    });
    const environment = createEnvironment({ endpoint: "/graphql", fetch: client });

    let thrown: mixed = null;
    try {
      await networkOf(environment).execute(operation, {}, {}).toPromise();
    } catch (error) {
      thrown = error;
    }

    expect((thrown: $FlowFixMe).errors.length).toBe(2);
    expect(String(thrown)).toContain("and 1 more");
  });

  it("passes a clean response straight through", async () => {
    const { client } = recording({ data: { viewer: { id: "1" } } });
    const environment = createEnvironment({ endpoint: "/graphql", fetch: client });

    const result = await networkOf(environment).execute(operation, {}, {}).toPromise();

    expect((result: $FlowFixMe).data).toEqual({ viewer: { id: "1" } });
  });
});
