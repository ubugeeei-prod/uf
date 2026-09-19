// @flow
// The facade returns the outer uf worker's registry, not another test runner.
const environment: $FlowFixMe = globalThis;
const api = environment.__UF_NATIVE_TEST_API__;
export const test = api.test;
export const it = api.it;
export const describe = api.describe;
export const beforeAll = api.beforeAll;
export const afterAll = api.afterAll;
export const beforeEach = api.beforeEach;
export const afterEach = api.afterEach;
export const expect = api.expect;
export const fn = api.fn;
export const spyOn = api.spyOn;
