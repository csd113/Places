// support/dom.mjs - Minimal DOM + canvas environment so the real editor scripts
// (renderer, editor, properties, app) can be booted and driven from node.
//
// This is intentionally small: it implements exactly the surface the editor uses,
// records every getElementById('…') call so the smoke test can prove that every
// requested id exists in index.html, and stubs the 2D canvas so drawing calls are
// no-ops.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
export const editorDir = path.resolve(here, '../..');

export function readIndexHtml() {
  return fs.readFileSync(path.join(editorDir, 'index.html'), 'utf8');
}

function makeClassList(el) {
  return {
    _set: new Set(),
    add(...names) { names.forEach(n => this._set.add(n)); },
    remove(...names) { names.forEach(n => this._set.delete(n)); },
    toggle(name, force) {
      const on = force === undefined ? !this._set.has(name) : !!force;
      if (on) this._set.add(name); else this._set.delete(name);
      return on;
    },
    contains(name) { return this._set.has(name); }
  };
}

class StubElement {
  constructor(tagName = 'div', id = '') {
    this.tagName = tagName.toUpperCase();
    this.id = id;
    this.dataset = {};
    this.style = {};
    this.children = [];
    this.hidden = false;
    this.disabled = false;
    this.value = '';
    this.checked = false;
    this.files = [];
    this.selectedIndex = 0;
    this.options = [];
    this.classList = makeClassList(this);
    this.listeners = new Map();
    this._html = '';
    this._text = '';
    this.type = this.tagName === 'INPUT' ? 'text' : undefined;
  }

  get innerHTML() { return this._html; }
  set innerHTML(value) { this._html = String(value); }
  get textContent() { return this._text; }
  set textContent(value) { this._text = String(value); }
  get className() { return Array.from(this.classList._set).join(' '); }
  set className(value) { this.classList._set = new Set(String(value).split(/\s+/).filter(Boolean)); }

  addEventListener(type, handler) {
    if (!this.listeners.has(type)) this.listeners.set(type, []);
    this.listeners.get(type).push(handler);
  }

  removeEventListener(type, handler) {
    const list = this.listeners.get(type) || [];
    const index = list.indexOf(handler);
    if (index >= 0) list.splice(index, 1);
  }

  /** Dispatches an event to this element and its ancestors. */
  dispatchEvent(event) {
    const type = event.type;
    event.target = event.target || this;
    for (const handler of this.listeners.get(type) || []) handler(event);
    if (this.parentElement) this.parentElement.dispatchEvent(event);
    return true;
  }

  click() {
    this.dispatchEvent({ type: 'click', target: this });
  }

  getBoundingClientRect() {
    return { left: 0, top: 0, width: 900, height: 600, right: 900, bottom: 600 };
  }

  getContext(kind) {
    if (kind !== '2d') return null;
    if (!this._ctx) this._ctx = createStubContext();
    return this._ctx;
  }

  toDataURL() { return 'data:image/png;base64,'; }

  querySelector(selector) { return matchAll(this, selector)[0] || null; }
  querySelectorAll(selector) { return matchAll(this, selector); }
  closest(selector) { return this; }
  focus() { this._focused = true; }
  appendChild(child) { child.parentElement = this; this.children.push(child); return child; }
  removeChild(child) {
    const index = this.children.indexOf(child);
    if (index >= 0) this.children.splice(index, 1);
    return child;
  }
  find(selector) {
    const parts = String(selector).split(/\s+/);
    const needle = parts[parts.length - 1];
    const [tag, id, cls] = parseSimple(needle);
    if (tag && this.tagName !== tag.toUpperCase()) return null;
    if (id && this.id !== id) return null;
    if (cls && !this.classList.contains(cls)) return null;
    return this;
  }
}

function parseSimple(part) {
  let tag = null, id = null, cls = null;
  let rest = part;
  const idMatch = rest.match(/#([\w-]+)/);
  if (idMatch) { id = idMatch[1]; rest = rest.replace(idMatch[0], ''); }
  const clsMatches = rest.match(/\.([\w-]+)/g);
  if (clsMatches) { cls = clsMatches[0].slice(1); }
  const tagPart = rest.replace(/\.[\w-]+/g, '').replace(/\[.*\]/g, '');
  if (tagPart) tag = tagPart;
  return [tag, id, cls];
}

/** Very small selector matcher covering the selectors the editor uses. */
function matchAll(root, selector) {
  const results = [];
  const selectors = selector.split(',').map(s => s.trim()).filter(Boolean);
  walk(root, (node) => {
    for (const simple of selectors) {
      if (matches(node, simple)) { results.push(node); return; }
    }
  });
  return results;
}

function walk(node, visit) {
  for (const child of node.children) {
    visit(child);
    walk(child, visit);
  }
}

function matches(node, selector) {
  for (const part of selector.split(/\s+/)) {
    if (part.includes('[')) {
      const attr = part.match(/\[([\w-]+)(?:="([^"]*)")?\]/);
      if (!attr) continue;
      const value = node.dataset ? node.dataset[attr[1]] : undefined;
      if (value === undefined) return false;
      if (attr[2] !== undefined && String(value) !== attr[2]) return false;
      continue;
    }
    const [tag, id, cls] = parseSimple(part);
    if (tag && tag !== '*' && node.tagName !== tag.toUpperCase()) return false;
    if (id && node.id !== id) return false;
    if (cls && !node.classList.contains(cls)) return false;
  }
  return true;
}

function createStubContext() {
  const gradient = { addColorStop() {} };
  const handler = {
    get(target, prop) {
      if (prop in target) return target[prop];
      if (prop === 'measureText') return () => ({ width: 10 });
      if (prop === 'createRadialGradient' || prop === 'createLinearGradient') return () => gradient;
      if (prop === 'createPattern') return () => ({});
      if (prop === 'createImageData') {
        return (w, h) => ({ width: Math.max(1, w | 0), height: Math.max(1, h | 0), data: new Uint8ClampedArray(Math.max(4, (w | 0) * (h | 0) * 4)) });
      }
      if (prop === 'getImageData') {
        return (x, y, w, h) => ({ width: Math.max(1, w | 0), height: Math.max(1, h | 0), data: new Uint8ClampedArray(Math.max(4, (w | 0) * (h | 0) * 4)) });
      }
      return () => {};
    },
    set(target, prop, value) { target[prop] = value; return true; }
  };
  return new Proxy({
    canvas: null,
    save() {}, restore() {}, setTransform() {}, scale() {}, translate() {}, rotate() {},
    fillRect() {}, strokeRect() {}, clearRect() {}, beginPath() {}, moveTo() {}, lineTo() {},
    arc() {}, ellipse() {}, closePath() {}, fill() {}, stroke() {}, setLineDash() {},
    fillText() {}, drawImage() {}, clip() {}, rect() {}, putImageData() {}
  }, handler);
}

/**
 * Creates a bootable browser-ish environment for the editor.
 * Returns { window, document, boot(), missingIds, flushFrame() }.
 */
export function createEnvironment(options = {}) {
  const html = readIndexHtml();
  const idsInHtml = new Set(Array.from(html.matchAll(/id="([^"]+)"/g)).map(m => m[1]));
  const requestedIds = new Set();
  const elementsById = new Map();
  const createdElements = [];

  // Honour `checked`/`disabled`/`value` attributes so the stub start state matches
  // the real markup (e.g. grid snapping is on by default).
  const initialAttributes = new Map();
  for (const tag of html.matchAll(/<input[^>]*>/g)) {
    const text = tag[0];
    const idMatch = text.match(/id="([^"]+)"/);
    if (!idMatch) continue;
    initialAttributes.set(idMatch[1], {
      checked: /\schecked/.test(text),
      disabled: /\sdisabled/.test(text),
      value: (text.match(/value="([^"]*)"/) || [])[1]
    });
  }

  const documentStub = {
    title: '',
    body: new StubElement('body'),
    documentElement: new StubElement('html'),
    getElementById(id) {
      requestedIds.add(id);
      if (!elementsById.has(id)) {
        // Provide an element anyway so a missing id surfaces as a recorded mismatch
        // instead of a crash in the middle of the boot sequence.
        const element = new StubElement('div', id);
        const initial = initialAttributes.get(id);
        if (initial) {
          element.checked = initial.checked;
          element.disabled = initial.disabled;
          if (initial.value !== undefined) element.value = initial.value;
          if (id === 'chk-snap' || id === 'chk-heights') element.type = 'checkbox';
          if (initial.value !== undefined && /^\d+$/.test(initial.value)) element.type = 'number';
        }
        elementsById.set(id, element);
      }
      return elementsById.get(id);
    },
    querySelector(selector) {
      if (selector === '.app') return elementsById.get('app-root') || new StubElement('div');
      if (selector === '#multi-dx' || selector === '#multi-dz') return new StubElement('input');
      return null;
    },
    querySelectorAll(selector) {
      if (selector.includes('data-view')) return [makeEl('button', { view: '2d' }), makeEl('button', { view: '3d' }), makeEl('button', { view: 'split' })];
      if (selector.includes('data-tool')) {
        return ['select', 'room', 'wall', 'door', 'window', 'light', 'prop', 'spawn', 'patch']
          .map(tool => makeEl('button', { tool }));
      }
      return [];
    },
    createElement(tag) {
      const element = new StubElement(tag);
      createdElements.push(element);
      return element;
    },
    addEventListener() {}
  };

  function makeEl(tag, dataset) {
    const element = new StubElement(tag);
    element.dataset = { ...dataset };
    return element;
  }

  const storage = new Map();
  const frames = [];
  const windowListeners = new Map();
  const windowStub = {
    devicePixelRatio: 1,
    addEventListener(type, handler) {
      if (!windowListeners.has(type)) windowListeners.set(type, []);
      windowListeners.get(type).push(handler);
    },
    removeEventListener(type, handler) {
      const list = windowListeners.get(type) || [];
      const index = list.indexOf(handler);
      if (index >= 0) list.splice(index, 1);
    },
    dispatchWindowEvent(event) {
      const type = event.type;
      for (const handler of windowListeners.get(type) || []) handler(event);
      return true;
    },
    requestAnimationFrame(callback) { frames.push(callback); return frames.length; },
    cancelAnimationFrame() {},
    localStorage: {
      getItem(key) { return storage.has(key) ? storage.get(key) : null; },
      setItem(key, value) { storage.set(key, String(value)); },
      removeItem(key) { storage.delete(key); }
    },
    alert() {},
    confirm() { return true; },
    setTimeout: (fn) => { frames.push(fn); return frames.length; },
    clearTimeout() {},
    navigator: { clipboard: { writeText: async () => {} } }
  };
  windowStub.window = windowStub;
  windowStub.document = documentStub;
  windowStub.globalThis = windowStub;
  windowStub.console = console;
  windowStub.Math = Math;
  windowStub.JSON = JSON;
  windowStub.Number = Number;
  windowStub.Object = Object;
  windowStub.Array = Array;
  windowStub.Set = Set;
  windowStub.Map = Map;
  windowStub.Error = Error;
  windowStub.Boolean = Boolean;
  windowStub.String = String;
  windowStub.Date = Date;
  windowStub.Promise = Promise;
  windowStub.Symbol = Symbol;
  windowStub.Proxy = Proxy;
  windowStub.Uint8Array = Uint8Array;
  windowStub.Float32Array = Float32Array;
  windowStub.isFinite = isFinite;
  windowStub.URL = URL;
  windowStub.Blob = Blob;
  windowStub.Image = class { set src(value) { this._src = value; } };
  windowStub.Event = class Event {
    constructor(type, options = {}) {
      this.type = type;
      this.bubbles = !!options.bubbles;
      this.target = null;
      this.defaultPrevented = false;
    }
    preventDefault() { this.defaultPrevented = true; }
    stopPropagation() {}
  };
  windowStub.CustomEvent = windowStub.Event;
  windowStub.FileReader = class { readAsDataURL() {} };
  windowStub.JSZip = undefined;

  const context = vm.createContext(windowStub);

  const scripts = [
    'js/jszip.min.js',
    'js/geometry.js', 'js/model.js', 'js/props.js', 'js/history.js', 'js/ops.js',
    'js/renderer.js', 'js/camera3d.js', 'js/viewport3d.js', 'js/properties.js',
    'js/io.js', 'js/editor.js', 'js/app.js'
  ];

  function loadScript(relative) {
    const full = path.join(editorDir, relative);
    if (!fs.existsSync(full)) return false;
    const source = fs.readFileSync(full, 'utf8');
    vm.runInContext(source, context, { filename: relative });
    return true;
  }

  return {
    window: windowStub,
    document: documentStub,
    idsInHtml,
    requestedIds,
    createdElements,
    loadScript,
    loadScripts(names) {
      for (const name of names) loadScript(name);
    },
    loadAll() {
      const loaded = [];
      for (const name of scripts) if (loadScript(name)) loaded.push(name);
      return loaded;
    },
    /** Runs queued requestAnimationFrame callbacks once. */
    flushFrame() {
      const queued = frames.splice(0, frames.length);
      for (const callback of queued) callback(performance.now ? performance.now() : 0);
    },
    /** Dispatches an event to the window listeners registered by the editor. */
    fireWindow(type, event = {}) {
      return windowStub.dispatchWindowEvent({ type, preventDefault() {}, ...event });
    },
    run(expression) {
      return vm.runInContext(expression, context);
    }
  };
}
