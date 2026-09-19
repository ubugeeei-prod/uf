/**
 * @generated SignedSource<<4d395aa5baebcc5d0542b85ed5b8fec9>>
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
    "cacheID": "8831307a65817d868bd47b694d5401cc",
    "id": null,
    "metadata": {},
    "name": "SnsUpdateSettingsMutation",
    "operationKind": "mutation",
    "text": "mutation SnsUpdateSettingsMutation(\n  $input: SettingsInput!\n) {\n  updateSettings(input: $input) {\n    id\n    displayName\n    bio\n    email\n  }\n}\n"
  }
};
})();

(node/*:: as any*/).hash = "2f2c8eee2b73ac47c170603c2613fb6f";

export default ((node/*:: as any*/)/*:: as Mutation<
  SnsUpdateSettingsMutation$variables,
  SnsUpdateSettingsMutation$data,
>*/);
