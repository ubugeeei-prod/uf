/**
 * @generated SignedSource<<56e257050858cb8caa1e6378d7de1588>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
import type { SnsThreads_query$fragmentType } from "./SnsThreads_query.graphql";
export type SnsThreadsRefetchQuery$variables = {};
export type SnsThreadsRefetchQuery$data = {
  readonly $fragmentSpreads: SnsThreads_query$fragmentType,
};
export type SnsThreadsRefetchQuery = {
  response: SnsThreadsRefetchQuery$data,
  variables: SnsThreadsRefetchQuery$variables,
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
    "name": "SnsThreadsRefetchQuery",
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
    "name": "SnsThreadsRefetchQuery",
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
    "cacheID": "323c82e815cd7f593f519a5c19bdf815",
    "id": null,
    "metadata": {},
    "name": "SnsThreadsRefetchQuery",
    "operationKind": "query",
    "text": "query SnsThreadsRefetchQuery {\n  ...SnsThreads_query\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsThread_thread on Thread {\n  id\n  lastMessage\n  participant {\n    name\n    ...SnsAvatar_user\n    id\n  }\n}\n\nfragment SnsThreads_query on Query {\n  threads {\n    id\n    ...SnsThread_thread\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "b2a6ed6704810daae46f74cc259a5230";

export default ((node/*:: as any*/)/*:: as Query<
  SnsThreadsRefetchQuery$variables,
  SnsThreadsRefetchQuery$data,
>*/);
