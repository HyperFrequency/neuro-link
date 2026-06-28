// SPDX-License-Identifier: MIT
//
// Minimal DOM stub for bun tests. Implements only the HTMLElement surface
// MessageList and Composer actually use (createDiv/createSpan/createEl,
// empty, remove, classList, dataset, addEventListener, querySelector,
// textContent). Not a full jsdom — we don't need layout, selection, or CSS.
//
// Install by calling `installDomStub()` once in a `beforeAll` block before
// the test imports DOM-touching code.

interface CreateOpts {
  text?: string;
  cls?: string;
  attr?: Record<string, string>;
}

interface StubElement {
  tagName: string;
  textContent: string;
  children: StubElement[];
  parent: StubElement | null;
  attributes: Record<string, string>;
  dataset: Record<string, string>;
  classList: ClassList;
  style: Record<string, string>;

  createDiv(opts?: CreateOpts): StubElement;
  createSpan(opts?: CreateOpts): StubElement;
  createEl(tag: string, opts?: CreateOpts): StubElement;
  empty(): void;
  remove(): void;
  addClass(cls: string): void;
  removeClass(cls: string): void;
  setText(v: string): void;
  setAttribute(k: string, v: string): void;
  insertBefore(node: StubElement, before: StubElement | null): void;
  addEventListener(_t: string, _cb: () => void): void;
  querySelector(sel: string): StubElement | null;
  querySelectorAll(sel: string): StubElement[];
  scrollTop: number;
  scrollHeight: number;
}

interface ClassList {
  add(cls: string): void;
  remove(cls: string): void;
  toggle(cls: string, on?: boolean): void;
  contains(cls: string): boolean;
}

function makeDataset(attributes: Record<string, string>): Record<string, string> {
  return new Proxy({} as Record<string, string>, {
    set(_target, prop, value) {
      const key = String(prop);
      const attrKey = `data-${toKebab(key)}`;
      attributes[attrKey] = String(value);
      return true;
    },
    get(_target, prop) {
      const key = String(prop);
      const attrKey = `data-${toKebab(key)}`;
      return attributes[attrKey];
    },
  });
}

function toKebab(camel: string): string {
  return camel.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

function createElement(tag: string): StubElement {
  const attrs: Record<string, string> = {};
  // Store direct textContent; `textContent` getter recurses children.
  let directText = "";
  const el: StubElement = {
    tagName: tag.toUpperCase(),
    get textContent() {
      // Mirror DOM: `textContent` returns the concatenation of all descendant
      // text nodes. We approximate by flattening direct text + child text.
      const childText = this.children.map((c) => c.textContent).join("");
      return directText + childText;
    },
    set textContent(v: string) {
      directText = v;
      // Setting textContent clears children in real DOM.
      for (const c of this.children) c.parent = null;
      this.children.length = 0;
    },
    children: [],
    parent: null,
    attributes: attrs,
    dataset: makeDataset(attrs),
    style: {},
    scrollTop: 0,
    scrollHeight: 0,
    classList: {
      add(cls) {
        const list = splitClass(attrs);
        if (!list.includes(cls)) list.push(cls);
        attrs["class"] = list.join(" ");
      },
      remove(cls) {
        attrs["class"] = splitClass(attrs).filter((c) => c !== cls).join(" ");
      },
      toggle(cls, on) {
        const has = this.contains(cls);
        const shouldAdd = on ?? !has;
        if (shouldAdd && !has) this.add(cls);
        if (!shouldAdd && has) this.remove(cls);
      },
      contains(cls) {
        return splitClass(attrs).includes(cls);
      },
    },
    createDiv(opts) {
      return el.createEl("div", opts);
    },
    createSpan(opts) {
      return el.createEl("span", opts);
    },
    createEl(childTag, opts = {}) {
      const child = createElement(childTag);
      if (opts.text !== undefined) child.textContent = opts.text;
      if (opts.cls) child.classList.add(opts.cls);
      if (opts.attr) {
        for (const [k, v] of Object.entries(opts.attr)) {
          child.setAttribute(k, v);
        }
      }
      child.parent = el;
      el.children.push(child);
      return child;
    },
    empty() {
      for (const c of el.children) c.parent = null;
      el.children.length = 0;
      el.textContent = "";
    },
    remove() {
      const p = el.parent;
      if (p) p.children = p.children.filter((c) => c !== el);
      el.parent = null;
    },
    addClass(cls) {
      el.classList.add(cls);
    },
    removeClass(cls) {
      el.classList.remove(cls);
    },
    setText(v) {
      el.textContent = v;
    },
    setAttribute(k, v) {
      attrs[k] = v;
      // Also mirror onto dataset if it's a data- attribute so both sides
      // stay coherent.
    },
    insertBefore(node, before) {
      if (!before) {
        el.children.push(node);
        node.parent = el;
        return;
      }
      const idx = el.children.indexOf(before);
      if (idx < 0) {
        el.children.push(node);
      } else {
        el.children.splice(idx, 0, node);
      }
      node.parent = el;
    },
    addEventListener: () => {
      /* no-op */
    },
    querySelector(sel) {
      return matchSelector(el, sel, true)[0] ?? null;
    },
    querySelectorAll(sel) {
      return matchSelector(el, sel, false);
    },
  };
  return el;
}

function splitClass(attrs: Record<string, string>): string[] {
  const raw = attrs["class"];
  return raw ? raw.split(/\s+/).filter(Boolean) : [];
}

function matchSelector(
  root: StubElement,
  sel: string,
  firstOnly: boolean
): StubElement[] {
  const predicate = compileSelector(sel);
  const out: StubElement[] = [];
  const stack: StubElement[] = [...root.children];
  while (stack.length > 0) {
    const node = stack.shift()!;
    if (predicate(node)) {
      out.push(node);
      if (firstOnly) break;
    }
    stack.push(...node.children);
  }
  return out;
}

function compileSelector(sel: string): (el: StubElement) => boolean {
  const attrMatch = sel.match(/^(.*?)\[([^=]+)="([^"]+)"\]$/);
  let head = sel;
  let attr: { name: string; value: string } | null = null;
  if (attrMatch) {
    head = attrMatch[1];
    attr = { name: attrMatch[2], value: attrMatch[3] };
  }
  const wantClass = head.startsWith(".") ? head.slice(1) : null;
  const wantTag = !wantClass && head ? head.toLowerCase() : null;

  return (el) => {
    if (wantClass && !el.classList.contains(wantClass)) return false;
    if (wantTag && el.tagName.toLowerCase() !== wantTag) return false;
    if (attr) {
      if (el.attributes[attr.name] !== attr.value) return false;
    }
    return true;
  };
}

let installed = false;

export function installDomStub(): void {
  if (installed) return;
  installed = true;
  const g = globalThis as unknown as {
    document?: { createElement: (tag: string) => StubElement };
    HTMLElement?: unknown;
  };
  g.document = { createElement };
  g.HTMLElement = class {};
}
