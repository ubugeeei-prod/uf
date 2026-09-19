/**
 * @generated SignedSource<<fe8503fd9b6993d74e937325368f07fa>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type SnsAppreciateMutation$variables = {
  id: string,
  liked: boolean,
};
export type SnsAppreciateMutation$data = {
  readonly setAppreciation: {
    readonly id: string,
    readonly liked: boolean,
    readonly likes: number,
  },
};
export type SnsAppreciateMutation = {
  response: SnsAppreciateMutation$data,
  variables: SnsAppreciateMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "id"
  },
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "liked"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "id",
        "variableName": "id"
      },
      {
        "kind": "Variable",
        "name": "liked",
        "variableName": "liked"
      }
    ],
    "concreteType": "Post",
    "kind": "LinkedField",
    "name": "setAppreciation",
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
        "name": "likes",
        "storageKey": null
      },
      {
        "alias": null,
        "args": null,
        "kind": "ScalarField",
        "name": "liked",
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
    "name": "SnsAppreciateMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsAppreciateMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "7dcb9790ed655f59900ee511beec4424",
    "id": null,
    "metadata": {},
    "name": "SnsAppreciateMutation",
    "operationKind": "mutation",
    "text": "mutation SnsAppreciateMutation(\n  $id: ID!\n  $liked: Boolean!\n) {\n  setAppreciation(id: $id, liked: $liked) {\n    id\n    likes\n    liked\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "b58c76a4cce3cd7139f457a4f90bfc69";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsAppreciateMutation$variables,
  SnsAppreciateMutation$data,
>*/);
