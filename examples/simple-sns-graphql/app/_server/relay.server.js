// @flow

import { headers } from "@uniflowed/server";
import { createServerEnvironment } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { environment } from "../_shared/relay-environment.js";
import sessionQuery from "../_shared/__generated__/SnsSessionQuery.graphql.js";

import type { SessionRef } from "../_shared/session.client.js";

import { endpoint, sessionCookie } from "./upstream.js";

// React.cache in Relay scopes this factory to one RSC request. A module-level
// Environment would share identities and private records between visitors.

export const relay = createServerEnvironment(() =>
  environment(endpoint(), sessionCookie(headers().get("cookie") ?? "")),
);

/**
 * Start the viewer read that the frame's session islands consume. A route that also gates
 * content on identity creates the reference once and hands the same one to both.
 */

export function preloadSession(): SessionRef {
  return relay.serverPreloadQuery(sessionQuery, {});
}
