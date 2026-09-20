// @flow
import { createFetch } from "@uniflowed/fetch";

export const client = createFetch({ fetch: async () => new Response("fetched") });
