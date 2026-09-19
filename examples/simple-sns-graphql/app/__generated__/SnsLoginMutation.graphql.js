/**
 * @generated SignedSource<<cbdeafc196769746838a9a1517701898>>
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
    readonly avatar: string,
    readonly bio: string,
    readonly handle: string,
    readonly id: string,
    readonly name: string,
    readonly photo: ?string,
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
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "name",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "handle",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "avatar",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "photo",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "bio",
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
    "cacheID": "4421863fee37796eed91a47d579f8b13",
    "id": null,
    "metadata": {},
    "name": "SnsLoginMutation",
    "operationKind": "mutation",
    "text": "mutation SnsLoginMutation(\n  $handle: String!\n  $password: String!\n) {\n  login(handle: $handle, password: $password) {\n    id\n    name\n    handle\n    avatar\n    photo\n    bio\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "7fe8679f6afbb7bcce9ffa126ca8bb55";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsLoginMutation$variables,
  SnsLoginMutation$data,
>*/);
