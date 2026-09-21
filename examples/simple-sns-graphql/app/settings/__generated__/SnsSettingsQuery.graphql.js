/**
 * @generated SignedSource<<11e03202c62d14ce6c44df6143bb07b7>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
export type SnsSettingsQuery$variables = {};
export type SnsSettingsQuery$data = {
  readonly settings: ?{
    readonly bio: string,
    readonly displayName: string,
    readonly email: string,
  },
};
export type SnsSettingsQuery = {
  response: SnsSettingsQuery$data,
  variables: SnsSettingsQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "displayName",
  "storageKey": null
},
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "bio",
  "storageKey": null
},
v2 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "email",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsSettingsQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Settings",
        "kind": "LinkedField",
        "name": "settings",
        "plural": false,
        "selections": [
          (v0/*:: as any*/),
          (v1/*:: as any*/),
          (v2/*:: as any*/)
        ],
        "storageKey": null
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "SnsSettingsQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Settings",
        "kind": "LinkedField",
        "name": "settings",
        "plural": false,
        "selections": [
          (v0/*:: as any*/),
          (v1/*:: as any*/),
          (v2/*:: as any*/),
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
    ]
  },
  "params": {
    "cacheID": "941044e739bb2c3ca656601d0646765e",
    "id": null,
    "metadata": {},
    "name": "SnsSettingsQuery",
    "operationKind": "query",
    "text": "query SnsSettingsQuery {\n  settings {\n    displayName\n    bio\n    email\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "7b6551f277ead548dbc3d789bcc39ed9";

export default ((node/*:: as any*/)/*:: as Query<
  SnsSettingsQuery$variables,
  SnsSettingsQuery$data,
>*/);
