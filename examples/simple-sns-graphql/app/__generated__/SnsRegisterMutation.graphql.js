/**
 * @generated SignedSource<<3576209464087df268314a3e600125f2>>
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
    readonly avatar: string,
    readonly bio: string,
    readonly handle: string,
    readonly id: string,
    readonly name: string,
    readonly photo: ?string,
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
    "cacheID": "cedc46ae5e0360112fb8379154c65317",
    "id": null,
    "metadata": {},
    "name": "SnsRegisterMutation",
    "operationKind": "mutation",
    "text": "mutation SnsRegisterMutation(\n  $input: RegisterInput!\n) {\n  register(input: $input) {\n    id\n    name\n    handle\n    avatar\n    photo\n    bio\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "520069a20c7ad1810065932d60772e82";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsRegisterMutation$variables,
  SnsRegisterMutation$data,
>*/);
