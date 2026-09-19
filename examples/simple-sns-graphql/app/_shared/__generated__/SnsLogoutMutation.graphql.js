/**
 * @generated SignedSource<<e501c0d5ee7eef575bfa8fd2a3726aa3>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type SnsLogoutMutation$variables = {};
export type SnsLogoutMutation$data = {
  readonly logout: boolean,
};
export type SnsLogoutMutation = {
  response: SnsLogoutMutation$data,
  variables: SnsLogoutMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "alias": null,
    "args": null,
    "kind": "ScalarField",
    "name": "logout",
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsLogoutMutation",
    "selections": (v0/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "SnsLogoutMutation",
    "selections": (v0/*:: as any*/)
  },
  "params": {
    "cacheID": "c261be86b5969002bf0eb8a71c6d8c42",
    "id": null,
    "metadata": {},
    "name": "SnsLogoutMutation",
    "operationKind": "mutation",
    "text": "mutation SnsLogoutMutation {\n  logout\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "69a0ce3508158fcc61715f74a8739b46";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsLogoutMutation$variables,
  SnsLogoutMutation$data,
>*/);
