/**
 * @generated SignedSource<<89ed130585d28a606f77826d612bb56f>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsAvatar_user$fragmentType: FragmentType;
export type SnsAvatar_user$data = {
  readonly avatar: string,
  readonly id: string,
  readonly photo: ?string,
  readonly $fragmentType: SnsAvatar_user$fragmentType,
};
export type SnsAvatar_user$key = {
  readonly $data?: SnsAvatar_user$data,
  readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsAvatar_user",
  "selections": [
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
  "type": "User",
  "abstractKey": null
};

(node/*:: as any*/).hash = "36318ac2eb2dc1145dbcd1d172a19015";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsAvatar_user$fragmentType,
  SnsAvatar_user$data,
>*/);
