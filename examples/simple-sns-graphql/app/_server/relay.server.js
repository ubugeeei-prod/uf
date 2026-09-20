// @flow

import { headers } from "@uniflowed/server";
import { createServerEnvironment } from "@uniflowed/relay/rsc_EXPERIMENTAL";

import { environment } from "../_shared/relay-environment.js";

import { endpoint, sessionCookie } from "./upstream.js";

// React.cache in Relay scopes this factory to one RSC request. A module-level
// Environment would share identities and private records between visitors.

export const relay = createServerEnvironment(() =>
  environment(endpoint(), sessionCookie(headers().get("cookie") ?? "")),
);
