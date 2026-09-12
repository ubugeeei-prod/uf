// @noflow
//
// Route-handler OpenAPI output.
//
// The build driver imports this module before it registers the Node Flow
// hooks, so it must stay plain JavaScript and must not statically import
// Flow-authored packages. The schema and method vocabulary are loaded inside
// `createOpenApiDocument`, after the driver has installed the hooks.

const JSON = "application/json";
const QUERY_EXTENSION = "x-uf-query";
const STANDARD_METHODS = new Set(["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"]);

export async function createOpenApiDocument(handlers, options = {}) {
  const [{ HANDLER_METHODS }, { toJsonSchema }] = await Promise.all([
    import("@uniflowed/router/handler"),
    import("@uniflowed/validator/json-schema"),
  ]);
  const paths = {};
  for (const record of handlers ?? []) {
    const routePath = openApiPath(record.path);
    const pathItem = paths[routePath] ?? {};
    paths[routePath] = pathItem;
    const module = await loadHandlerModule(record, pathItem);
    if (module == null) continue;
    for (const method of implementedMethods(module, HANDLER_METHODS)) {
      const operation = createOperation(record, method, schemaFor(module, method), toJsonSchema);
      if (STANDARD_METHODS.has(method)) {
        pathItem[method.toLowerCase()] = operation;
      } else if (method === "QUERY") {
        pathItem[QUERY_EXTENSION] = operation;
      }
    }
  }
  return {
    openapi: "3.1.0",
    info: {
      title: options.title ?? "uf application",
      version: options.version ?? "0.0.0",
    },
    paths,
  };
}

async function loadHandlerModule(record, pathItem) {
  try {
    return await record.load();
  } catch (error) {
    if (record.file != null) pathItem["x-uf-source"] = record.file;
    pathItem["x-uf-schema-unavailable"] = errorMessage(error);
    return null;
  }
}

function implementedMethods(module, methods) {
  return methods.filter(
    (method) =>
      typeof module[method] === "function" ||
      (method === "HEAD" && typeof module.GET === "function"),
  );
}

function schemaFor(module, method) {
  const table = module.schemas;
  if (!isRecord(table)) return null;
  const schema = table[method];
  if (schema == null) return null;
  if (!isRecord(schema)) {
    throw new Error(`route handler schemas.${method} must be an object`);
  }
  return schema;
}

function createOperation(record, method, schema, toJsonSchema) {
  const parameters = pathParameters(record);
  const operation = {
    operationId: operationId(record, method),
    responses: untypedResponses(method),
  };
  if (record.file != null) operation["x-uf-source"] = record.file;
  if (parameters.length > 0) operation.parameters = parameters;
  if (schema == null) {
    operation["x-uf-untyped"] = true;
    return operation;
  }

  const unrepresentable = [];
  if (schema.query != null) {
    addQuerySchema(operation, exportSchema(toJsonSchema, schema.query, "query", unrepresentable));
  }
  if (schema.body != null) {
    operation.requestBody = {
      required: true,
      content: {
        [JSON]: { schema: exportSchema(toJsonSchema, schema.body, "body", unrepresentable) },
      },
    };
  }
  if (schema.response != null) {
    operation.responses = {
      "200": {
        description: method === "HEAD" ? "Typed response headers" : "Typed response",
        content:
          method === "HEAD"
            ? undefined
            : {
                [JSON]: {
                  schema: exportSchema(toJsonSchema, schema.response, "response", unrepresentable),
                },
              },
      },
    };
    if (method === "HEAD") delete operation.responses["200"].content;
  }
  if (unrepresentable.length > 0) {
    operation["x-uf-unrepresentable"] = unrepresentable;
  }
  return operation;
}

function untypedResponses(method) {
  return {
    "200": {
      description: method === "HEAD" ? "Untyped response headers" : "Untyped response",
    },
  };
}

function exportSchema(toJsonSchema, schema, part, unrepresentable) {
  const exported = toJsonSchema(schema);
  for (const item of exported.unrepresentable) {
    unrepresentable.push({ part, path: item.path, kind: item.kind });
  }
  return stripDialect(exported.schema);
}

function addQuerySchema(operation, schema) {
  if (schema.type !== "object" || !isRecord(schema.properties) || hasOwn(schema, "$defs")) {
    operation["x-uf-query-schema"] = schema;
    return;
  }
  const required = new Set(Array.isArray(schema.required) ? schema.required : []);
  const parameters = Object.keys(schema.properties).map((name) => ({
    name,
    in: "query",
    required: required.has(name),
    schema: schema.properties[name],
  }));
  operation.parameters = [...(operation.parameters ?? []), ...parameters];
}

function stripDialect(value) {
  if (Array.isArray(value)) return value.map(stripDialect);
  if (!isRecord(value)) return value;
  const out = {};
  for (const key of Object.keys(value)) {
    if (key === "$schema") continue;
    out[key] = stripDialect(value[key]);
  }
  return out;
}

function openApiPath(routePath) {
  return routePath
    .split("/")
    .map((segment) => {
      if (!segment.startsWith(":")) return segment;
      return `{${segment.endsWith("*") ? segment.slice(1, -1) : segment.slice(1)}}`;
    })
    .join("/");
}

function pathParameters(record) {
  const params = record.params ?? paramsFromPath(record.path);
  return params.map((param) => {
    const parameter = {
      name: param.name,
      in: "path",
      required: true,
      schema: { type: "string" },
    };
    if (param.catchAll) {
      parameter.description = "Catch-all route segment, slash-separated in the URL.";
    }
    return parameter;
  });
}

function paramsFromPath(routePath) {
  return routePath
    .split("/")
    .filter((segment) => segment.startsWith(":"))
    .map((segment) => ({
      name: segment.endsWith("*") ? segment.slice(1, -1) : segment.slice(1),
      catchAll: segment.endsWith("*"),
    }));
}

function operationId(record, method) {
  const name = `${method.toLowerCase()} ${record.path}`;
  return name
    .replaceAll(/[^a-zA-Z0-9]+/g, " ")
    .trim()
    .replaceAll(/ ([a-zA-Z0-9])/g, (_, char) => char.toUpperCase());
}

function isRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function hasOwn(value, key) {
  return Object.prototype.hasOwnProperty.call(value, key);
}

function errorMessage(error) {
  if (error instanceof Error) return error.message;
  return String(error);
}
