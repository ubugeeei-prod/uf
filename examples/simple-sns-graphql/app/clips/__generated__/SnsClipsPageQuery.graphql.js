/**
 * @generated SignedSource<<a566a9a75ee82e6fbe5d16a79ad457d0>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsSocialFrame_query$fragmentType } from "./../../_shared/__generated__/SnsSocialFrame_query.graphql";
export type SnsClipsPageQuery$variables = {};
export type SnsClipsPageQuery$data = {
  readonly $fragmentSpreads: SnsSocialFrame_query$fragmentType,
};
export type SnsClipsPageQuery = {
  response: SnsClipsPageQuery$data,
  variables: SnsClipsPageQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsClipsPageQuery",
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
    "name": "SnsClipsPageQuery",
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
    "cacheID": "d78add9914fa0c9c9f5f1238a878e70e",
    "id": null,
    "metadata": {},
    "name": "SnsClipsPageQuery",
    "operationKind": "query",
    "text": "query SnsClipsPageQuery {\n  ...SnsSocialFrame_query\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};

(node/*:: as any*/).hash = "e5d97d6f01f4e2add441741f0dee5b87";

export default ((node/*:: as any*/)/*:: as Query<
  SnsClipsPageQuery$variables,
  SnsClipsPageQuery$data,
>*/);
