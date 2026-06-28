// SPDX-License-Identifier: MIT
//
// jsdom-style tests for MessageList. We set up the minimum DOM we need via
// a tiny HTMLElement stub (Bun doesn't ship jsdom by default). The stub
// backs only the methods MessageList calls; anything unexpected throws so
// we catch accidental coupling.
//
// What we're asserting:
//   - append() creates a bubble per message, keyed by id.
//   - update() re-renders the body of a single bubble without destroying
//     the rest.
//   - remove() drops a bubble from the DOM + internal map.
//   - toolCalls render as collapsible blocks.
//   - Role-specific classes applied.

import "./_obsidian-mock";
import { describe, expect, test, beforeAll } from "bun:test";
import { installDomStub } from "./_dom-stub";
import { MessageList, type ChatMessage } from "../src/views/message-list";
import type { App, Component } from "obsidian";

beforeAll(() => {
  installDomStub();
});

function makeParent(): HTMLElement {
  return document.createElement("div") as unknown as HTMLElement;
}

function makeApp(): App {
  return {} as unknown as App;
}

function makeComponent(): Component {
  return { register: () => {}, registerEvent: () => {} } as unknown as Component;
}

describe("MessageList", () => {
  test("append() creates a bubble with role classes and content", () => {
    const parent = makeParent();
    const list = new MessageList(parent, {
      app: makeApp(),
      parent: makeComponent(),
      autoScroll: false,
    });
    const msg: ChatMessage = {
      id: "m1",
      role: "user",
      content: "hello world",
      timestamp: Date.now(),
    };
    const bubble = list.append(msg);
    expect(bubble.classList.contains("nlr-chat-message-user")).toBe(true);
    expect(bubble.dataset.messageId).toBe("m1");
    // Content bubbles with role "user" render as pre.
    const body = bubble.querySelector(".nlr-chat-message-body") as HTMLElement;
    expect(body.textContent).toContain("hello world");
  });

  test("update() re-renders only the body", () => {
    const parent = makeParent();
    const list = new MessageList(parent, {
      app: makeApp(),
      parent: makeComponent(),
      autoScroll: false,
    });
    list.append({
      id: "m2",
      role: "assistant",
      content: "v1",
      timestamp: 1,
    });
    list.append({
      id: "m3",
      role: "user",
      content: "unrelated",
      timestamp: 2,
    });
    list.update("m2", "v2 updated");

    const bubble = parent.querySelector('[data-message-id="m2"]') as HTMLElement;
    const body = bubble.querySelector(".nlr-chat-message-body") as HTMLElement;
    expect(body.textContent).toContain("v2 updated");

    // Unrelated bubble untouched.
    const other = parent.querySelector('[data-message-id="m3"]') as HTMLElement;
    expect(other).toBeTruthy();
  });

  test("remove() drops the bubble", () => {
    const parent = makeParent();
    const list = new MessageList(parent, {
      app: makeApp(),
      parent: makeComponent(),
      autoScroll: false,
    });
    list.append({ id: "m4", role: "user", content: "x", timestamp: 0 });
    list.remove("m4");
    expect(parent.querySelector('[data-message-id="m4"]')).toBe(null);
  });

  test("toolCalls render as collapsible blocks with outcome classes", () => {
    const parent = makeParent();
    const list = new MessageList(parent, {
      app: makeApp(),
      parent: makeComponent(),
      autoScroll: false,
    });
    list.append({
      id: "m5",
      role: "assistant",
      content: "done",
      timestamp: 0,
      toolCalls: [
        { id: "t1", name: "tv_search", arguments: "{}", outcome: "ok" },
        { id: "t2", name: "tv_write_note", arguments: "{}", outcome: "refused" },
      ],
    });
    const bubble = parent.querySelector('[data-message-id="m5"]') as HTMLElement;
    const calls = bubble.querySelectorAll(".nlr-chat-tool-call");
    expect(calls.length).toBe(2);
    // outcome-specific class
    expect((calls[1] as HTMLElement).classList.contains("nlr-chat-tool-call-refused")).toBe(true);
  });

  test("append is idempotent on duplicate id", () => {
    const parent = makeParent();
    const list = new MessageList(parent, {
      app: makeApp(),
      parent: makeComponent(),
      autoScroll: false,
    });
    const a = list.append({ id: "x", role: "user", content: "a", timestamp: 0 });
    const b = list.append({ id: "x", role: "user", content: "b", timestamp: 0 });
    expect(a).toBe(b);
    expect(parent.querySelectorAll('[data-message-id="x"]').length).toBe(1);
  });
});
