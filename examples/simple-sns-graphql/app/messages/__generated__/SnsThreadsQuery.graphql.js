/**
 * @generated SignedSource<<7d21a31c0fd5428d41d1564070702bbc>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsThread_thread$fragmentType } from "./SnsThread_thread.graphql";
export type SnsThreadsQuery$variables = {};
export type SnsThreadsQuery$data = {
  readonly threads: ReadonlyArray<{
    readonly id: string,
    readonly $fragmentSpreads: SnsThread_thread$fragmentType,
  }>,
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
        "alias": null,
        "args": null,
        "concreteType": "Thread",
        "kind": "LinkedField",
        "name": "threads",
        "plural": true,
        "selections": [
          (v0/*:: as any*/),
          {
            "args": null,
            "kind": "FragmentSpread",
            "name": "SnsThread_thread"
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
    "cacheID": "5c2bf7eb8e5f38dc7a4957ecdf9a6b70",
    "id": null,
    "metadata": {},
    "name": "SnsThreadsQuery",
    "operationKind": "query",
    "text": "query SnsThreadsQuery {\n  threads {\n    id\n    ...SnsThread_thread\n  }\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsThread_thread on Thread {\n  id\n  lastMessage\n  participant {\n    name\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "efb208a9c85a4b4b59bfcdf03d3316f0";

export default ((node/*:: as any*/)/*:: as Query<
  SnsThreadsQuery$variables,
  SnsThreadsQuery$data,
>*/);
