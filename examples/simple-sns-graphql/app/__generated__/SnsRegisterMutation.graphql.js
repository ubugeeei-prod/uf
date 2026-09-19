/**
 * @generated SignedSource<<b978aad866737e8ed1b99cf61f365ee9>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type RegisterInput = {
  email: string,
  handle: string,
  name: string,
  password: string,
};
export type SnsRegisterMutation$variables = {
  input: RegisterInput,
};
export type SnsRegisterMutation$data = {
  readonly register: {
    readonly id: string,
  },
};
export type SnsRegisterMutation = {
  response: SnsRegisterMutation$data,
  variables: SnsRegisterMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      }
    ],
    "concreteType": "User",
    "kind": "LinkedField",
    "name": "register",
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
    "name": "SnsRegisterMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsRegisterMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "ec21c76443ba88758a7d92ad8b2a8633",
    "id": null,
    "metadata": {},
    "name": "SnsRegisterMutation",
    "operationKind": "mutation",
    "text": "mutation SnsRegisterMutation(\n  $input: RegisterInput!\n) {\n  register(input: $input) {\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "c7078c71de970de480a325ce125a9d29";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsRegisterMutation$variables,
  SnsRegisterMutation$data,
>*/);
