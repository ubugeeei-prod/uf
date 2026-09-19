/**
 * @generated SignedSource<<de5873400a23a5750fca8959b146e044>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsSettings_query$fragmentType: FragmentType;
export type SnsSettings_query$data = {
  readonly settings: ?{
    readonly bio: string,
    readonly displayName: string,
    readonly email: string,
  },
  readonly $fragmentType: SnsSettings_query$fragmentType,
};
export type SnsSettings_query$key = {
  readonly $data?: SnsSettings_query$data,
  readonly $fragmentSpreads: SnsSettings_query$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsSettings_query",
  "selections": [
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
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "f5b85ee384d14f501705af15b50d464f";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsSettings_query$fragmentType,
  SnsSettings_query$data,
>*/);
