/**
 * @generated SignedSource<<2100b486dbbd8c8814c125a6d83d7468>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Query } from 'relay-runtime';
import type { SnsSettings_query$fragmentType } from "./SnsSettings_query.graphql";
import type { SnsSocialFrame_query$fragmentType } from "./../../_shared/__generated__/SnsSocialFrame_query.graphql";
export type SnsSettingsPageQuery$variables = {};
export type SnsSettingsPageQuery$data = {
  readonly $fragmentSpreads: SnsSettings_query$fragmentType & SnsSocialFrame_query$fragmentType,
};
export type SnsSettingsPageQuery = {
  response: SnsSettingsPageQuery$data,
  variables: SnsSettingsPageQuery$variables,
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
    "name": "SnsSettingsPageQuery",
    "selections": [
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsSocialFrame_query"
      },
      {
        "args": null,
        "kind": "FragmentSpread",
        "name": "SnsSettings_query"
      }
    ],
    "type": "Query",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": [],
    "kind": "Operation",
    "name": "SnsSettingsPageQuery",
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
      },
      {
        "alias": null,
        "args": null,
        "concreteType": "Settings",
        "kind": "LinkedField",
        "name": "settings",
        "plural": false,
        "selections": [
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "displayName",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "bio",
            "storageKey": null
          },
          {
            "alias": null,
            "args": null,
            "kind": "ScalarField",
            "name": "email",
            "storageKey": null
          },
          (v0/*:: as any*/)
        ],
        "storageKey": null
      }
    ]
  },
  "params": {
    "cacheID": "f0dd30e33620e2e5c5e26ef65067be8b",
    "id": null,
    "metadata": {},
    "name": "SnsSettingsPageQuery",
    "operationKind": "query",
    "text": "query SnsSettingsPageQuery {\n  ...SnsSocialFrame_query\n  ...SnsSettings_query\n}\n\nfragment SnsAvatar_user on User {\n  id\n  photo\n  avatar\n}\n\nfragment SnsSettings_query on Query {\n  settings {\n    displayName\n    bio\n    email\n    id\n  }\n}\n\nfragment SnsSocialFrame_query on Query {\n  viewer {\n    name\n    handle\n    ...SnsAvatar_user\n    id\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "36ab57f3fee1eeb91b0b9c14273d0334";

export default ((node/*:: as any*/)/*:: as Query<
  SnsSettingsPageQuery$variables,
  SnsSettingsPageQuery$data,
>*/);
