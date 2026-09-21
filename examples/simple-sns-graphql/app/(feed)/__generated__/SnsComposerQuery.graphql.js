/**
 * @generated SignedSource<<daff94e6a5650b9b31a4ccdcf4d45988>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsComposer_viewer$fragmentType } from "./SnsComposer_viewer.graphql";
export type SnsComposerQuery$variables = {};
export type SnsComposerQuery$data = {
  readonly viewer: ?{
    readonly $fragmentSpreads: SnsComposer_viewer$fragmentType,
  },
};
export type SnsComposerQuery = {
  response: SnsComposerQuery$data,
  variables: SnsComposerQuery$variables,
};
*/

var node/*: ConcreteRequest*/ = {
  "fragment": {
    "argumentDefinitions": [],
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsComposerQuery",
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
            "args": null,
            "kind": "FragmentSpread",
            "name": "SnsComposer_viewer"
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
    "name": "SnsComposerQuery",
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
    "cacheID": "d9dfa1327aac801009af099e7eea6e94",
    "id": null,
    "metadata": {},
    "name": "SnsComposerQuery",
    "operationKind": "query",
    "text": "query SnsComposerQuery {\n  viewer {\n    ...SnsComposer_viewer\n    id\n  }\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsComposer_viewer on User {\n  name\n  ...SnsAvatar_user\n}\n"
  }
};

(node/*:: as any*/).hash = "80e8e6d8a04397c5eb13f87e0e9e6253";

export default ((node/*:: as any*/)/*:: as Query<
  SnsComposerQuery$variables,
  SnsComposerQuery$data,
>*/);
