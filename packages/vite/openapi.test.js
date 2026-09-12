// @flow

import { describe, expect, it } from "@uniflowed/test";
import { number, object, optional, string } from "@uniflowed/validator";

import { createOpenApiDocument } from "./internal/openapi.js";

function handler(
  path: string,
  params: $ReadOnlyArray<{| name: string, catchAll: boolean |}>,
  module: { readonly [string]: mixed, ... },
) {
  return {
    path,
    params,
    file: `app${path}/$route.js`,
    load: async () => module,
  };
}

describe("route handler OpenAPI", () => {
  it("turns method schemas into OpenAPI operations", async () => {
    const document = await createOpenApiDocument([
      handler("/api/users/:id", [{ name: "id", catchAll: false }], {
        GET() {},
        POST() {},
        schemas: {
          GET: {
            query: object({ expand: optional(string()) }),
            response: object({ id: string(), age: number() }),
          },
          POST: {
            body: object({ name: string() }),
            response: object({ id: string() }),
          },
        },
      }),
    ]);

    expect(document.openapi).toBe("3.1.0");
    expect(document.paths["/api/users/{id}"].get.parameters).toEqual([
      {
        name: "id",
        in: "path",
        required: true,
        schema: { type: "string" },
      },
      {
        name: "expand",
        in: "query",
        required: false,
        schema: { type: "string" },
      },
    ]);
    expect(document.paths["/api/users/{id}"].get.responses["200"]).toEqual({
      description: "Typed response",
      content: {
        "application/json": {
          schema: {
            type: "object",
            properties: { id: { type: "string" }, age: { type: "number" } },
            required: ["id", "age"],
          },
        },
      },
    });
    expect(document.paths["/api/users/{id}"].post.requestBody).toEqual({
      required: true,
      content: {
        "application/json": {
          schema: {
            type: "object",
            properties: { name: { type: "string" } },
            required: ["name"],
          },
        },
      },
    });
  });

  it("lists a handler with no schema as untyped", async () => {
    const document = await createOpenApiDocument([
      handler("/api/health", [], {
        GET() {},
      }),
    ]);

    expect(document.paths["/api/health"].get["x-uf-untyped"]).toBe(true);
    expect(document.paths["/api/health"].get.responses).toEqual({
      "200": { description: "Untyped response" },
    });
    expect(document.paths["/api/health"].head["x-uf-untyped"]).toBe(true);
  });

  it("keeps a handler whose module cannot be loaded in the route list", async () => {
    const document = await createOpenApiDocument([
      {
        path: "/api/native",
        params: [],
        file: "app/api/native/$route.js",
        load: async () => {
          throw new Error('Unknown file extension ".node"');
        },
      },
    ]);

    expect(document.paths["/api/native"]).toEqual({
      "x-uf-source": "app/api/native/$route.js",
      "x-uf-schema-unavailable": 'Unknown file extension ".node"',
    });
  });

  it("keeps QUERY out of the standard OpenAPI operation keys", async () => {
    const document = await createOpenApiDocument([
      handler("/api/search", [], {
        QUERY() {},
        schemas: {
          QUERY: {
            body: object({ q: string() }),
            response: object({ total: number() }),
          },
        },
      }),
    ]);

    expect(document.paths["/api/search"].query).toBe(undefined);
    expect(document.paths["/api/search"]["x-uf-query"].responses["200"]).toEqual({
      description: "Typed response",
      content: {
        "application/json": {
          schema: {
            type: "object",
            properties: { total: { type: "number" } },
            required: ["total"],
          },
        },
      },
    });
  });
});
