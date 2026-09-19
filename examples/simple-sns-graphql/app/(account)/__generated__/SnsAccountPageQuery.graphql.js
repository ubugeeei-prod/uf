/**
 * @generated SignedSource<<2cfebee99e4474a94be2eb23a6d4c9ff>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsSocialFrame_query$fragmentType } from "./../../_shared/__generated__/SnsSocialFrame_query.graphql";
export type SnsAccountPageQuery$variables = {};
export type SnsAccountPageQuery$data = {
  readonly $fragmentSpreads: SnsSocialFrame_query$fragmentType,
};
export type SnsAccountPageQuery = {
  response: SnsAccountPageQuery$data,
  variables: SnsAccountPageQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsAccountPageQuery",
    "selections": [
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsSocialFrame_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "SnsAccountPageQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "User",
        "kind": "LinkedField",
        "name": "viewer",
        "plural": false,
        "selections": [
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
    "cacheID": "fd9ae5615c5465f8adb6fc31a27c3d87",
    "id": null,
    "metadata": {},
    "name": "SnsAccountPageQuery",
    "operationKind": "query",
    "text": "query SnsAccountPageQuery {\n  ...SnsSocialFrame_query\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};

(node/*:: as any*/).hash = "3c1a0e73ec7ecc20f92fba2bdcaf80d0";

export default ((node/*:: as any*/)/*:: as Query<
  SnsAccountPageQuery$variables,
  SnsAccountPageQuery$data,
>*/);
