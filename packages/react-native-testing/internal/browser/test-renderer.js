// @flow
//
// Browser replacement for the Node-only optional renderer loader.

export function loadTestRenderer(): empty {
  const error: $FlowFixMe = new Error("Cannot find module 'react-test-renderer'");
  error.code = "MODULE_NOT_FOUND";
  throw error;
}
