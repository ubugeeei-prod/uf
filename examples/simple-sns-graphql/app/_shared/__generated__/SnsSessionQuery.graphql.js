/**
 * @generated SignedSource<<81666f6987e704dcea1c852a22cafac3>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsAvatar_user$fragmentType } from "./SnsAvatar_user.graphql";
export type SnsSessionQuery$variables = {};
export type SnsSessionQuery$data = {
  readonly viewer: ?{
    readonly handle: string,
    readonly name: string,
    readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  },
};
export type SnsSessionQuery = {
  response: SnsSessionQuery$data,
  variables: SnsSessionQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "name",
  "storageKey": null
},
v1 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "handle",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsSessionQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": [
          (v0/*:: as any*/),
          (v1/*:: as any*/),
          {
            "args": null,
            "kind": "FragmentSpread",
            "name": "SnsAvatar_user"
          }
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
    "name": "SnsSessionQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": [
          (v0/*:: as any*/),
          (v1/*:: as any*/),
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
            "name": "photo",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "avatar",
            "storageKey": null
          }
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "934066be496b34b76a36615064535dc1",
    "id": null,
    "metadata": {},
    "name": "SnsSessionQuery",
    "operationKind": "query",
    "text": "query SnsSessionQuery {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "357e5491f4ee0863f09f260ab3653b10";

export default ((node/*:: as any*/)/*:: as Query<
  SnsSessionQuery$variables,
  SnsSessionQuery$data,
>*/);
