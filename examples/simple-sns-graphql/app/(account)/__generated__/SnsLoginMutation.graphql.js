/**
 * @generated SignedSource<<e6807635b2f6edff0185a7aa3377ec25>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type SnsLoginMutation$variables = {
  handle: string,
  password: string,
};
export type SnsLoginMutation$data = {
  readonly login: {
    readonly id: string,
  },
};
export type SnsLoginMutation = {
  response: SnsLoginMutation$data,
  variables: SnsLoginMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "handle"
  },
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "password"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "handle",
        "variableName": "handle"
      },
      {
        "kind": "Variable",
        "name": "password",
        "variableName": "password"
      }
    ],
    "concreteType": "User",
    "kind": "LinkedField",
    "name": "login",
    "plural": false,
    "selections": [
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "id",
        "storageKey": null
      }
    ],
    "storageKey": null
  }
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsLoginMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsLoginMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "55c5c4039d288ce854dc183b39a47975",
    "id": null,
    "metadata": {},
    "name": "SnsLoginMutation",
    "operationKind": "mutation",
    "text": "mutation SnsLoginMutation(\n  $handle: String!\n  $password: String!\n) {\n  login(handle: $handle, password: $password) {\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "513307a3899e1052fb49994a2bd33262";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsLoginMutation$variables,
  SnsLoginMutation$data,
>*/);
