// props.js - Prop catalog (registry) for the Liminal level editor.
//
// The catalog is data, not code: adding a model later means adding an entry to
// `assets/props/props.json` (and, once a mesh pipeline exists, pointing the entry's
// `model` field at the asset). The editor never hard-codes stove/sink behaviour.
//
// Catalog entry shape (shared with the game's `PropCatalog` in src/loader.rs):
//   { "id": "core:couch", "name": "Couch", "category": "Furniture",
//     "size": [2.0, 0.9, 0.9], "color": "#6b5f4a", "model": null, "solid": true }
// `size` is the full box extent in metres ([width, height, depth]) with the box
// resting on the prop's base; `model` is the future mesh asset path.
//
// The built-in list below mirrors `assets/props/props.json` so the editor keeps
// working from file:// where fetch() is unavailable. `tests/props.test.mjs` asserts
// the two stay in sync.

(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LiminalProps = api;
    root.PropCatalog = api.PropCatalog;
    root.PROP_CATEGORIES = api.PROP_CATEGORIES;
    root.BUILTIN_PROPS = api.BUILTIN_PROPS;
  }
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  const PROP_CATEGORIES = ['Furniture', 'Appliances', 'Utility', 'Decorative', 'Other'];
  const PROP_FALLBACK_SIZE = [0.6, 0.9, 0.6];
  const PROP_FALLBACK_COLOR = [0.54, 0.53, 0.5];

  const BUILTIN_PROPS = [
    { id: 'core:couch', name: 'Couch', category: 'Furniture', size: [2.0, 0.9, 0.9], color: '#6b5f4a', model: 'models/couch.glb', solid: true },
    { id: 'core:armchair', name: 'Armchair', category: 'Furniture', size: [0.9, 0.9, 0.9], color: '#7a6a55', model: null, solid: true },
    { id: 'core:chair', name: 'Chair', category: 'Furniture', size: [0.5, 0.9, 0.5], color: '#8a7a63', model: null, solid: true },
    { id: 'core:table', name: 'Table', category: 'Furniture', size: [1.4, 0.75, 0.8], color: '#6f5a41', model: null, solid: true },
    { id: 'core:desk', name: 'Desk', category: 'Furniture', size: [1.6, 0.75, 0.7], color: '#5f5142', model: null, solid: true },
    { id: 'core:bookshelf', name: 'Bookshelf', category: 'Furniture', size: [1.0, 1.8, 0.35], color: '#57452f', model: null, solid: true },
    { id: 'core:cabinet', name: 'Cabinet', category: 'Furniture', size: [0.9, 0.85, 0.45], color: '#6a5c4a', model: null, solid: true },
    { id: 'core:bed', name: 'Bed', category: 'Furniture', size: [1.4, 0.55, 2.0], color: '#7d7568', model: null, solid: true },
    { id: 'core:stove', name: 'Stove', category: 'Appliances', size: [0.6, 0.9, 0.6], color: '#8f8a80', model: null, solid: true },
    { id: 'core:sink', name: 'Sink', category: 'Appliances', size: [0.6, 0.85, 0.55], color: '#9aa0a0', model: null, solid: true },
    { id: 'core:fridge', name: 'Fridge', category: 'Appliances', size: [0.7, 1.8, 0.7], color: '#b8bcc0', model: null, solid: true },
    { id: 'core:washing_machine', name: 'Washing Machine', category: 'Appliances', size: [0.6, 0.85, 0.6], color: '#a2a6aa', model: null, solid: true },
    { id: 'core:vending_machine', name: 'Vending Machine', category: 'Appliances', size: [1.0, 1.9, 0.8], color: '#4f5a63', model: null, solid: true },
    { id: 'core:water_cooler', name: 'Water Cooler', category: 'Appliances', size: [0.35, 1.1, 0.35], color: '#8fa4ae', model: null, solid: true },
    { id: 'core:crate', name: 'Crate', category: 'Utility', size: [0.6, 0.6, 0.6], color: '#7a6244', model: null, solid: true },
    { id: 'core:cardboard_box', name: 'Cardboard Box', category: 'Other', size: [0.5, 0.5, 0.5], color: '#a8895f', model: null, solid: false },
    { id: 'core:plant', name: 'Plant', category: 'Decorative', size: [0.4, 1.0, 0.4], color: '#4f6b43', model: null, solid: false },
    { id: 'core:rug', name: 'Rug', category: 'Decorative', size: [2.0, 0.02, 1.4], color: '#6d5a52', model: null, solid: false },
    { id: 'core:lamp', name: 'Floor Lamp', category: 'Decorative', size: [0.35, 1.5, 0.35], color: '#8a8272', model: null, solid: false },
    { id: 'core:tv', name: 'Television', category: 'Decorative', size: [1.1, 0.7, 0.1], color: '#33363a', model: 'models/tv.glb', solid: false }
  ];

  function parseHexColor(value) {
    if (typeof value !== 'string') return null;
    const hex = value.trim().replace(/^#/, '');
    if (!/^[0-9a-fA-F]{6}$/.test(hex)) return null;
    return [
      parseInt(hex.slice(0, 2), 16) / 255,
      parseInt(hex.slice(2, 4), 16) / 255,
      parseInt(hex.slice(4, 6), 16) / 255
    ];
  }

  function toHexColor(rgb) {
    if (!Array.isArray(rgb) || rgb.length < 3) return '#8a8a8a';
    return '#' + rgb.slice(0, 3).map(v => {
      const c = Math.max(0, Math.min(255, Math.round(Number(v) * 255)));
      return c.toString(16).padStart(2, '0');
    }).join('');
  }

  function normalizeEntry(raw, id) {
    if (!raw || typeof raw !== 'object') return null;
    const entryId = String(raw.id || id || '').trim();
    if (!entryId) return null;
    let size = Array.isArray(raw.size) && raw.size.length === 3 ? raw.size.map(Number) : null;
    if (!size || !size.every(v => Number.isFinite(v) && v > 0)) size = PROP_FALLBACK_SIZE.slice();
    const color = parseHexColor(raw.color) || PROP_FALLBACK_COLOR.slice();
    const category = PROP_CATEGORIES.includes(raw.category) ? raw.category : 'Other';
    return {
      id: entryId,
      name: String(raw.name || entryId.split(':').pop() || entryId),
      category,
      size,
      color,
      model: typeof raw.model === 'string' && raw.model.trim() ? raw.model.trim() : null,
      solid: raw.solid === true
    };
  }

  /** Parses a catalog JSON object ({props:[...]}) or an array of entries. Never throws. */
  function parsePropCatalog(data) {
    const list = [];
    const byId = new Map();
    const raw = Array.isArray(data) ? data : (data && Array.isArray(data.props) ? data.props : []);
    for (const item of raw) {
      const entry = normalizeEntry(item);
      if (entry && !byId.has(entry.id)) {
        byId.set(entry.id, entry);
        list.push(entry);
      }
    }
    return new PropCatalog(list, byId);
  }

  class PropCatalog {
    constructor(list, byId) {
      this.list = list || [];
      this.byId = byId || new Map();
    }

    static builtin() {
      return parsePropCatalog(BUILTIN_PROPS);
    }

    static fromJSON(data) {
      return parsePropCatalog(data);
    }

    get(id) {
      const key = String(id || '');
      const found = this.byId.get(key);
      if (found) return found;
      return {
        id: key,
        name: key.split(':').pop() || key || 'Prop',
        category: 'Other',
        size: PROP_FALLBACK_SIZE.slice(),
        color: PROP_FALLBACK_COLOR.slice(),
        model: null,
        solid: false,
        missing: true
      };
    }

    has(id) {
      return this.byId.has(String(id || ''));
    }

    get size() {
      return this.list.length;
    }

    categories() {
      const present = new Set(this.list.map(e => e.category));
      return PROP_CATEGORIES.filter(c => present.has(c)).concat(
        Array.from(present).filter(c => !PROP_CATEGORIES.includes(c)).sort()
      );
    }

    /** Entry search: matches name/id, optionally restricted to a category. */
    search(query, category) {
      const q = String(query || '').trim().toLowerCase();
      return this.list.filter(entry => {
        if (category && category !== 'All' && entry.category !== category) return false;
        if (!q) return true;
        return entry.name.toLowerCase().includes(q) || entry.id.toLowerCase().includes(q);
      });
    }
  }

  const DEFAULT_CATALOG_URLS = [
    '../assets/props/props.json',
    '../../assets/props/props.json',
    'assets/props/props.json',
    'props.json'
  ];

  /**
   * Loads the shared prop catalog, falling back to the built-in list when the file
   * cannot be fetched (file:// editing, missing file, invalid JSON). Never rejects.
   */
  async function loadPropCatalog(urls, fetchImpl) {
    const candidates = Array.isArray(urls) && urls.length ? urls : DEFAULT_CATALOG_URLS;
    const doFetch = fetchImpl || (typeof fetch === 'function' ? fetch : null);
    if (doFetch) {
      for (const url of candidates) {
        try {
          const response = await doFetch(url, { cache: 'no-store' });
          if (!response || !response.ok) continue;
          const data = await response.json();
          const catalog = parsePropCatalog(data);
          if (catalog.size > 0) {
            catalog.source = url;
            return catalog;
          }
        } catch (err) {
          // Try the next candidate; the built-in list is always available.
        }
      }
    }
    const builtin = PropCatalog.builtin();
    builtin.source = 'builtin';
    return builtin;
  }

  return {
    PROP_CATEGORIES,
    PROP_FALLBACK_SIZE,
    PROP_FALLBACK_COLOR,
    BUILTIN_PROPS,
    DEFAULT_CATALOG_URLS,
    PropCatalog,
    parsePropCatalog,
    parseHexColor,
    toHexColor,
    loadPropCatalog
  };
});
