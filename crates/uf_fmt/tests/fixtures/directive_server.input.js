"use server";
// @flow
import {db} from '@uniflowed/orm';

export async function createUser( name : string ) : Promise< void > {
'use strict';
await db.user.insert({ name , createdAt : Date.now() });
}
