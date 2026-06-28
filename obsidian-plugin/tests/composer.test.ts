// SPDX-License-Identifier: MIT
//
// Unit tests for the composer's pure helpers — specifically the `@`-token
// matcher. The DOM path of the composer (overlay rendering, keyboard
// dispatch) is exercised by users; we test the part that's interesting to
// get wrong.

import { describe, expect, test } from "bun:test";
import { matchAtToken } from "../src/views/composer";

describe("matchAtToken", () => {
  test("captures the token between @ and caret", () => {
    expect(matchAtToken("hello @foo", 10)).toEqual({ query: "foo", kind: "file" });
  });

  test("classifies @neuro as agent kind", () => {
    expect(matchAtToken("@neuro", 6)?.kind).toBe("agent");
    expect(matchAtToken("@Neuro", 6)?.kind).toBe("agent");
  });

  test("token must be preceded by whitespace or start-of-string", () => {
    // `foo@bar` at caret 7 — the '@' is preceded by 'o', so no match (email-safe).
    expect(matchAtToken("foo@bar", 7)).toBe(null);
    // Start-of-string case:
    expect(matchAtToken("@foo", 4)?.query).toBe("foo");
  });

  test("whitespace between @ and caret cancels the match", () => {
    expect(matchAtToken("@ foo", 5)).toBe(null);
  });

  test("empty query (caret just after @) still matches", () => {
    expect(matchAtToken("prefix @", 8)).toEqual({ query: "", kind: "file" });
  });

  test("no @ in the preceding text returns null", () => {
    expect(matchAtToken("some text here", 14)).toBe(null);
  });

  test("multiple @s — uses the one nearest the caret", () => {
    // "a @foo b @bar" with caret at end of @bar
    const txt = "a @foo b @bar";
    const pos = txt.length;
    expect(matchAtToken(txt, pos)).toEqual({ query: "bar", kind: "file" });
  });
});
