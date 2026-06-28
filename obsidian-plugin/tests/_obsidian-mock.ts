// SPDX-License-Identifier: MIT
//
// Shared obsidian-module mock for bun tests. Import this at the top of
// any test file that transitively loads a plugin source file importing
// from "obsidian" — the obsidian package at runtime is types-only (empty
// main), so without this shim the imports fail at load time.
//
// `mock.module` applies for the rest of the test run, so the *first* test
// file that imports this shim wins. To avoid a partial mock from one file
// leaking into another and breaking exports the second file doesn't know
// about, keep this list exhaustive for every symbol referenced from
// src/**/*.ts.

import { mock } from "bun:test";

mock.module("obsidian", () => {
  class NoticeStub {
    constructor(_msg: string, _timeout?: number) { /* no-op */ }
  }
  class TFileStub {}
  class ModalStub {
    constructor(_app: unknown) { /* no-op */ }
    open(): void { /* no-op */ }
    close(): void { /* no-op */ }
  }
  class AppStub {}
  class PluginStub {}
  class PluginSettingTabStub {
    constructor(_app: unknown, _plugin: unknown) { /* no-op */ }
  }
  class WorkspaceLeafStub {}
  class ItemViewStub {
    constructor(_leaf: unknown) { /* no-op */ }
  }
  class ComponentStub {
    registerEvent(): void { /* no-op */ }
    register(): void { /* no-op */ }
    load(): void { /* no-op */ }
    unload(): void { /* no-op */ }
  }
  class SettingStub {
    setName(): this { return this; }
    setDesc(): this { return this; }
    addText(): this { return this; }
    addButton(): this { return this; }
    addToggle(): this { return this; }
    addDropdown(): this { return this; }
    addTextArea(): this { return this; }
  }
  class TextComponentStub {}
  class ButtonComponentStub {}
  class DropdownComponentStub {}
  class MarkdownRendererStub {
    static render(_app: unknown, markdown: string, el: { textContent?: string }): Promise<void> {
      // Minimal jsdom-ish stub: drop raw markdown into textContent so tests
      // can assert content made it to the element.
      el.textContent = markdown;
      return Promise.resolve();
    }
  }
  const addIcon = (_id: string, _svg: string): void => { /* no-op */ };
  const setIcon = (_el: unknown, _icon: string): void => { /* no-op */ };
  return {
    Notice: NoticeStub,
    TFile: TFileStub,
    Modal: ModalStub,
    App: AppStub,
    Plugin: PluginStub,
    PluginSettingTab: PluginSettingTabStub,
    Setting: SettingStub,
    TextComponent: TextComponentStub,
    ButtonComponent: ButtonComponentStub,
    DropdownComponent: DropdownComponentStub,
    WorkspaceLeaf: WorkspaceLeafStub,
    ItemView: ItemViewStub,
    Component: ComponentStub,
    MarkdownRenderer: MarkdownRendererStub,
    addIcon,
    setIcon,
  };
});
