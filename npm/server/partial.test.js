// @flow
//
// A read of the request, while `uf build` prerenders a page's static shell.
//
// `cookies()`, `headers()` and `draftMode()` throw outside a request, and a
// partial prerender changes which exception: the read is recorded and thrown as
// a `PostponedReadError`, which the router turns into a hole a server fills per
// request (ubugeeei-prod/uf#950). Everything else about the three is what it
// was, and that is most of what is asserted below.

import { describe, expect, it } from "@uniflowed/test";
import {
  OutsideRequestError,
  PostponedReadError,
  cookies,
  draftMode,
  headers,
  nonce,
  requestId,
} from "@uniflowed/server";
import {
  contextFor,
  isPostponedRead,
  newPartialPrerender,
  runPartialPrerender,
  runWithContext,
} from "@uniflowed/server/host";

/** What `read` throws, or `null` if it returns. */
function thrownBy(read: () => mixed): mixed {
  try {
    read();
    return null;
  } catch (error) {
    return error;
  }
}

describe("a read during a partial prerender", () => {
  it("is recorded and thrown as a read left for the request, for each of the three", () => {
    const scope = newPartialPrerender();
    const thrown = runPartialPrerender(scope, () => [
      thrownBy(() => cookies()),
      thrownBy(() => headers()),
      thrownBy(() => draftMode()),
    ]);
    for (const error of thrown) {
      expect(error).toBeInstanceOf(PostponedReadError);
      expect(isPostponedRead(error)).toBe(true);
    }
    expect(scope.reads).toEqual(["cookies", "headers", "draftMode"]);
  });

  it("names the binding it was", () => {
    const error = runPartialPrerender(newPartialPrerender(), () => thrownBy(() => cookies()));
    expect(error instanceof PostponedReadError ? error.binding : null).toBe("cookies");
    expect(String(error)).toContain("cookies()");
  });

  it("follows the read across an await", async () => {
    const scope = newPartialPrerender();
    const error = await runPartialPrerender(scope, async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 0);
      });
      return thrownBy(() => headers());
    });
    expect(isPostponedRead(error)).toBe(true);
    expect(scope.reads).toEqual(["headers"]);
  });

  it("leaves `nonce()` and `requestId()` refused as they were", () => {
    const scope = newPartialPrerender();
    runPartialPrerender(scope, () => {
      expect(thrownBy(() => nonce())).toBeInstanceOf(OutsideRequestError);
      expect(thrownBy(() => requestId())).toBeInstanceOf(OutsideRequestError);
    });
    expect(scope.reads).toEqual([]);
  });

  it("answers from the request when there is one", () => {
    const request = new Request("https://uniflowed.dev/", { headers: { cookie: "session=ada" } });
    const scope = newPartialPrerender();
    const session = runPartialPrerender(scope, () =>
      runWithContext(contextFor(request), () => cookies().get("session")),
    );
    expect(session).toBe("ada");
    expect(scope.reads).toEqual([]);
  });
});

describe("a read outside one", () => {
  it("is still a read outside a request", () => {
    const error = thrownBy(() => cookies());
    expect(error).toBeInstanceOf(OutsideRequestError);
    expect(isPostponedRead(error)).toBe(false);
  });

  it("is not mistaken for a read left for the request", () => {
    expect(isPostponedRead(new Error("cookies"))).toBe(false);
    expect(isPostponedRead(null)).toBe(false);
    expect(isPostponedRead("PostponedReadError")).toBe(false);
  });
});
