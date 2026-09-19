/**
 * @generated SignedSource<<030d1f0ebf3e82c54c26f2ca9982c41c>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { ConcreteRequest, Mutation } from 'relay-runtime';
export type SettingsInput = {
  bio: string,
  displayName: string,
  email: string,
};
export type SnsUpdateSettingsMutation$variables = {
  input: SettingsInput,
};
export type SnsUpdateSettingsMutation$data = {
  readonly updateSettings: {
    readonly bio: string,
    readonly displayName: string,
    readonly email: string,
    readonly handle: string,
    readonly id: string,
  },
};
export type SnsUpdateSettingsMutation = {
  response: SnsUpdateSettingsMutation$data,
  variables: SnsUpdateSettingsMutation$variables,
};
*/

var node/*: ConcreteRequest*/ = (function(){
var v0 = [
  {
    "defaultValue": null,
    "kind": "LocalArgument",
    "name": "input"
  }
],
v1 = [
  {
    "alias": null,
    "args": [
      {
        "kind": "Variable",
        "name": "input",
        "variableName": "input"
      }
    ],
    "concreteType": "Settings",
    "kind": "LinkedField",
    "name": "updateSettings",
    "plural": false,
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
        "name": "displayName",
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
];
return {
  "fragment": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Fragment",
    "metadata": null,
    "name": "SnsUpdateSettingsMutation",
    "selections": (v1/*:: as any*/),
    "type": "Mutation",
    "abstractKey": null
  },
  "kind": "Request",
  "operation": {
    "argumentDefinitions": (v0/*:: as any*/),
    "kind": "Operation",
    "name": "SnsUpdateSettingsMutation",
    "selections": (v1/*:: as any*/)
  },
  "params": {
    "cacheID": "730ab9899b76cf3f9c51b0f9eed859da",
    "id": null,
    "metadata": {},
    "name": "SnsUpdateSettingsMutation",
    "operationKind": "mutation",
    "text": "mutation SnsUpdateSettingsMutation(\n  $input: SettingsInput!\n) {\n  updateSettings(input: $input) {\n    id\n    displayName\n    handle\n    bio\n    email\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "8ee5ef691e00b7f8ee84b6e34610a113";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsUpdateSettingsMutation$variables,
  SnsUpdateSettingsMutation$data,
>*/);
