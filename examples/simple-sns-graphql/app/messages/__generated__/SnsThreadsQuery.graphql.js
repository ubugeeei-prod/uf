/**
 * @generated SignedSource<<2f5993eec982323b81b1d22ad069f1ae>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsThreads_query$fragmentType } from "./SnsThreads_query.graphql";
export type SnsThreadsQuery$variables = {};
export type SnsThreadsQuery$data = {
  readonly $fragmentSpreads: SnsThreads_query$fragmentType,
};
export type SnsThreadsQuery = {
  response: SnsThreadsQuery$data,
  variables: SnsThreadsQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
};
return {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsThreadsQuery",
    "selections": [
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsThreads_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "SnsThreadsQuery",
    "selections": [
      {
        "alias": null,
        "args": null,
        "concreteType": "Thread",
        "kind": "LinkedField",
        "name": "threads",
        "plural": true,
        "selections": [
          (v0/*:: as any*/),
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "lastMessage",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "concreteType": "User",
            "kind": "LinkedField",
            "name": "participant",
            "plural": false,
            "selections": [
              {
                "alias": null,
                "args": null,
                "kind": "ScalarField",
                "name": "name",
                "storageKey": null
              },
              (v0/*:: as any*/),
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
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "69720a8c048f3fa7a9ecefd66c9d89f5",
    "id": null,
    "metadata": {},
    "name": "SnsThreadsQuery",
    "operationKind": "query",
    "text": "query SnsThreadsQuery {\n  ...SnsThreads_query\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsThread_thread on Thread {\n  id\n  lastMessage\n  participant {\n    name\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsThreads_query on Query {\n  threads {\n    id\n    ...SnsThread_thread\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "5321cc0a5277ef7ca116f8a4576f55f0";

export default ((node/*:: as any*/)/*:: as Query<
  SnsThreadsQuery$variables,
  SnsThreadsQuery$data,
>*/);
