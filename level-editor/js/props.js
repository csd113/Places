// props.js - Placeable asset catalog (registry) for the Places level editor.
//
// The catalog is data, not code: adding a model means adding an entry to
// `assets/catalog.json` and pointing its `model` field at the GLB asset. The
// editor never hard-codes stove/sink behaviour. Only placeable entries (asset
// type `prop` or `entity`) appear in the prop browser; materials, lights and
// decals are ignored by this legacy editor.
//
// Catalog entry shape (shared with the game's `PropCatalog` in src/loader.rs):
//   { "id": "core:couch", "display_name": "Couch", "asset_class": "environment",
//     "asset_type": "prop", "source": "file", "category": "Furniture",
//     "size": [2.0, 0.9, 0.9], "color": "#6b5f4a",
//     "model": "core/props/models/couch.glb", "solid": true }
// `size` is the full box extent in metres ([width, height, depth]) with the box
// resting on the prop's base; `model` is the canonical resource path relative to
// `assets/`. The editor treats the optional class/theme/type metadata as opaque.
//
// The built-in list below mirrors the placeable entries of `assets/catalog.json`
// so the editor keeps working from file:// where fetch() is unavailable.
// `tests/props.test.mjs` asserts the two stay in sync.
//
// Modelled props also get derived proxy geometry from
// `assets/prop_proxies.json` (written by tools/props/build.py). Loading it
// is optional in exactly the same way: with no proxy the catalogue box is drawn.

(function (root, factory) {
  const api = factory();
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.PlacesProps = api;
    root.PropCatalog = api.PropCatalog;
    root.PropProxies = api.PropProxies;
    root.PROP_CATEGORIES = api.PROP_CATEGORIES;
    root.BUILTIN_PROPS = api.BUILTIN_PROPS;
  }
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  const PROP_CATEGORIES = ['Furniture', 'Appliances', 'Utility', 'Decorative', 'Other'];
  const PROP_FALLBACK_SIZE = [0.6, 0.9, 0.6];
  const PROP_FALLBACK_COLOR = [0.54, 0.53, 0.5];

  const BUILTIN_PROPS = [
    { id: 'core:couch', name: 'Couch', category: 'Furniture', size: [2.0, 0.9, 0.9], color: '#6b5f4a', model: 'core/props/models/couch.glb', solid: true },
    { id: 'core:armchair', name: 'Armchair', category: 'Furniture', size: [0.9, 0.9, 0.9], color: '#7a6a55', model: 'core/props/models/armchair.glb', solid: true },
    { id: 'core:chair', name: 'Chair', category: 'Furniture', size: [0.5, 0.9, 0.5], color: '#6d6a63', model: 'environment/office/props/models/chair.glb', solid: true },
    { id: 'core:table', name: 'Table', category: 'Furniture', size: [1.4, 0.75, 0.8], color: '#6f5a41', model: 'core/props/models/table.glb', solid: true },
    { id: 'core:desk', name: 'Desk', category: 'Furniture', size: [1.6, 0.75, 0.7], color: '#3a3d42', model: 'environment/office/props/models/desk.glb', solid: true },
    { id: 'core:bookshelf', name: 'Bookshelf', category: 'Furniture', size: [1.0, 1.8, 0.35], color: '#57452f', model: 'core/props/models/bookshelf.glb', solid: true },
    { id: 'core:cabinet', name: 'Cabinet', category: 'Furniture', size: [0.9, 0.85, 0.45], color: '#6a5c4a', model: 'environment/office/props/models/cabinet.glb', solid: true },
    { id: 'core:bed', name: 'Bed', category: 'Furniture', size: [1.4, 0.55, 2.0], color: '#7d7568', model: 'core/props/models/bed.glb', solid: true },
    { id: 'core:stove', name: 'Stove', category: 'Appliances', size: [0.6, 0.9, 0.6], color: '#8f8a80', model: 'core/props/models/stove.glb', solid: true },
    { id: 'core:sink', name: 'Sink', category: 'Appliances', size: [0.6, 1.1, 0.55], color: '#9aa0a0', model: 'core/props/models/sink.glb', solid: true },
    { id: 'core:fridge', name: 'Fridge', category: 'Appliances', size: [0.7, 1.8, 0.7], color: '#b8bcc0', model: 'core/props/models/fridge.glb', solid: true },
    { id: 'core:washing_machine', name: 'Washing Machine', category: 'Appliances', size: [0.6, 0.85, 0.6], color: '#a2a6aa', model: 'core/props/models/washing_machine.glb', solid: true },
    { id: 'core:washer_drum', name: 'Washer Drum', category: 'Appliances', size: [0.42, 0.3, 0.42], color: '#b4b8ba', model: 'core/props/models/washer_drum.glb', solid: false },
    { id: 'core:vending_machine', name: 'Vending Machine', category: 'Appliances', size: [1.0, 1.9, 0.8], color: '#4f5a63', model: 'environment/office/props/models/vending_machine.glb', solid: true },
    { id: 'core:water_cooler', name: 'Water Cooler', category: 'Appliances', size: [0.35, 1.1, 0.35], color: '#8fa4ae', model: 'environment/office/props/models/water_cooler.glb', solid: true },
    { id: 'core:pool_table', name: 'Pool Patio Table', category: 'Furniture', size: [0.8, 0.74, 0.8], color: '#e8e6e0', model: 'environment/pool/props/models/pool_table.glb', solid: true },
    { id: 'core:pool_chair', name: 'Pool Patio Chair', category: 'Furniture', size: [0.52, 0.85, 0.55], color: '#e8e6e0', model: 'environment/pool/props/models/pool_chair.glb', solid: true },
    { id: 'core:pool_ladder', name: 'Pool Ladder', category: 'Utility', size: [0.55, 2.2, 0.45], color: '#b9bec2', model: 'environment/pool/props/models/pool_ladder.glb', solid: true },
    { id: 'core:pool_curtain_straight', name: 'Pool Curtain, Straight', category: 'Decorative', size: [1.2, 2.6, 0.22], color: '#dfe4e2', model: 'environment/pool/props/models/pool_curtain_straight.glb', solid: false },
    { id: 'core:pool_curtain_end', name: 'Pool Curtain, End', category: 'Decorative', size: [0.6, 2.6, 0.22], color: '#dfe4e2', model: 'environment/pool/props/models/pool_curtain_end.glb', solid: false },
    { id: 'core:pool_curtain_corner', name: 'Pool Curtain, Corner', category: 'Decorative', size: [0.6, 2.6, 0.6], color: '#dfe4e2', model: 'environment/pool/props/models/pool_curtain_corner.glb', solid: false },
    { id: 'core:pool_guardrail_straight', name: 'Pool Guardrail, Straight', category: 'Utility', size: [2.0, 1.05, 0.08], color: '#b9bec2', model: 'environment/pool/props/models/pool_guardrail_straight.glb', solid: true },
    { id: 'core:pool_guardrail_end', name: 'Pool Guardrail, End', category: 'Utility', size: [0.6, 1.05, 0.08], color: '#b9bec2', model: 'environment/pool/props/models/pool_guardrail_end.glb', solid: true },
    { id: 'core:pool_guardrail_corner', name: 'Pool Guardrail, Corner', category: 'Utility', size: [0.6, 1.05, 0.6], color: '#b9bec2', model: 'environment/pool/props/models/pool_guardrail_corner.glb', solid: true },
    { id: 'core:crate', name: 'Crate', category: 'Utility', size: [0.6, 0.6, 0.6], color: '#7a6244', model: 'core/props/models/crate.glb', solid: true },
    { id: 'core:cardboard_box', name: 'Cardboard Box', category: 'Other', size: [0.5, 0.5, 0.5], color: '#a8895f', model: 'core/props/models/cardboard_box.glb', solid: false },
    { id: 'core:plant', name: 'Plant', category: 'Decorative', size: [0.4, 1.0, 0.4], color: '#4f6b43', model: 'core/props/models/plant.glb', solid: false },
    { id: 'core:rug', name: 'Rug', category: 'Decorative', size: [2.0, 0.02, 1.4], color: '#6d5a52', model: 'core/props/models/rug.glb', solid: false },
    { id: 'core:lamp', name: 'Floor Lamp', category: 'Decorative', size: [0.35, 1.5, 0.35], color: '#8a8272', model: 'core/props/models/lamp.glb', solid: false },
    { id: 'core:tv', name: 'Television', category: 'Decorative', size: [1.1, 0.7, 0.1], color: '#33363a', model: 'core/props/models/tv.glb', solid: false },
    { id: 'spooner-man', name: 'Spooner-Man', category: 'Decorative', size: [0.27, 0.39, 1.02], color: '#33343a', model: 'entities/spooner-man/model/spooner-man.glb', solid: false },
    { id: 'home:cabinet_base', name: 'Base Kitchen Cabinet', category: 'Furniture', size: [0.6, 0.9, 0.6], color: '#d8d3c8', model: 'environment/home/props/models/cabinet_base.glb', solid: true },
    { id: 'home:cabinet_wall', name: 'Wall Kitchen Cabinet', category: 'Furniture', size: [0.6, 0.72, 0.33], color: '#ddd8ce', model: 'environment/home/props/models/cabinet_wall.glb', solid: true }
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

  /** True for the catalog entries the editor can place (props and entities). */
  function isPlaceableAsset(raw) {
    if (!raw || typeof raw !== 'object') return false;
    const type = raw.asset_type;
    if (type === undefined || type === null || type === '') return true; // legacy `props` entries
    return type === 'prop' || type === 'entity';
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
      name: String(raw.display_name || raw.name || entryId.split(':').pop() || entryId),
      category,
      size,
      color,
      model: typeof raw.model === 'string' && raw.model.trim() ? raw.model.trim() : null,
      solid: raw.solid === true
    };
  }

  /** Parses a catalog JSON object ({assets:[...]} or legacy {props:[...]}) or an array of entries. Never throws. */
  function parsePropCatalog(data) {
    const list = [];
    const byId = new Map();
    const source = Array.isArray(data)
      ? data
      : (data && (Array.isArray(data.assets) ? data.assets : (Array.isArray(data.props) ? data.props : [])));
    const raw = source.filter(isPlaceableAsset);
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
    '../assets/catalog.json',
    '../../assets/catalog.json',
    'assets/catalog.json',
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

  // ------------------------------------------------------------ proxy geometry
  //
  // `assets/prop_proxies.json` is derived from the shipped GLBs by
  // `tools/props/build.py` (never edited by hand) and gives the editor a compact,
  // flat-shaded stand-in for every modelled prop. Coordinates are metres in
  // prop-local space (origin at the floor contact, +Z front); `parts` mixes
  // box/cylinder/tube/plane entries whose colours are base `#rrggbb` values.

  const PLANE_NORMALS = ['y', 'z', '-z', 'x', '-x'];

  function asVec3(value) {
    if (!Array.isArray(value) || value.length < 3) return null;
    const out = value.slice(0, 3).map(Number);
    return out.every((v) => Number.isFinite(v)) ? out : null;
  }

  /** Validates and normalises one proxy part; null when it cannot be drawn. */
  function normalizeProxyPart(raw) {
    if (!raw || typeof raw !== 'object') return null;
    const shape = String(raw.shape || '');
    const color = parseHexColor(raw.color) || PROP_FALLBACK_COLOR.slice();

    if (shape === 'box') {
      const center = asVec3(raw.center);
      const size = asVec3(raw.size);
      if (!center || !size || !size.every((v) => v > 0)) return null;
      const rotation = asVec3(raw.rotation) || [0, 0, 0];
      return { shape, center, size, rotation, color };
    }

    if (shape === 'cylinder') {
      const base = asVec3(raw.base);
      const radius = Number(raw.radius);
      const height = Number(raw.height);
      if (!base || !Number.isFinite(radius) || radius <= 0 ||
          !Number.isFinite(height) || height <= 0) return null;
      const axis = raw.axis === 'x' || raw.axis === 'z' ? raw.axis : 'y';
      const segments = Math.max(3, Math.min(24, Math.round(Number(raw.segments)) || 8));
      const taper = Number(raw.taper) > 0 ? Number(raw.taper) : 1;
      return { shape, axis, base, radius, height, segments, taper, color };
    }

    if (shape === 'tube') {
      const start = asVec3(raw.start);
      const end = asVec3(raw.end);
      const radius = Number(raw.radius);
      if (!start || !end || !Number.isFinite(radius) || radius <= 0) return null;
      const length = Math.hypot(end[0] - start[0], end[1] - start[1], end[2] - start[2]);
      if (length <= 1e-6) return null;
      return { shape, start, end, radius, color };
    }

    if (shape === 'plane') {
      const center = asVec3(raw.center);
      const size = asVec3(raw.size);
      if (!center || !size) return null;
      const normal = PLANE_NORMALS.includes(raw.normal) ? raw.normal : 'y';
      const axes = normal === 'y' ? [0, 2] : (normal === 'x' || normal === '-x' ? [2, 1] : [0, 1]);
      if (!(size[axes[0]] > 0) || !(size[axes[1]] > 0)) return null;
      return { shape, center, size, normal, color };
    }

    return null;
  }

  function normalizeProxy(raw, id) {
    if (!raw || typeof raw !== 'object') return null;
    const proxyId = String(raw.id || id || '').trim();
    if (!proxyId) return null;
    const parts = [];
    for (const part of (Array.isArray(raw.parts) ? raw.parts : [])) {
      const normalized = normalizeProxyPart(part);
      if (normalized) parts.push(normalized);
    }
    if (!parts.length) return null;
    const texture = Array.isArray(raw.texture) && raw.texture.length === 2 &&
      raw.texture.every((value) => Number.isFinite(Number(value)) && Number(value) > 0)
      ? raw.texture.map(Number)
      : null;
    return {
      id: proxyId,
      name: String(raw.name || proxyId.split(':').pop() || proxyId),
      model: typeof raw.model === 'string' && raw.model.trim() ? raw.model.trim() : null,
      // Asset metadata generated alongside the geometry: the editor shows it
      // and the tests assert the pack's budgets through the real parser.
      triangles: Number.isFinite(Number(raw.triangles)) ? Math.max(0, Math.round(Number(raw.triangles))) : null,
      texture,
      boundsMin: asVec3(raw.bounds_min),
      boundsMax: asVec3(raw.bounds_max),
      parts
    };
  }

  class PropProxies {
    constructor(list, byId) {
      this.list = list || [];
      this.byId = byId || new Map();
    }

    static empty() {
      return new PropProxies([], new Map());
    }

    static fromJSON(data) {
      return parsePropProxies(data);
    }

    get(id) {
      return this.byId.get(String(id || '')) || null;
    }

    has(id) {
      return this.byId.has(String(id || ''));
    }

    get size() {
      return this.list.length;
    }
  }

  /** Parses a proxy file ({props:{id:{...}}}) or an array of proxy entries. Never throws. */
  function parsePropProxies(data) {
    let source = null;
    if (Array.isArray(data)) source = data;
    else if (data && Array.isArray(data.props)) source = data.props;
    else if (data && data.props && typeof data.props === 'object') source = data.props;

    const list = [];
    const byId = new Map();
    if (!source) return new PropProxies(list, byId);

    const entries = Array.isArray(source)
      ? source.map((item) => [item && item.id, item])
      : Object.keys(source).map((key) => [key, source[key]]);

    for (const [id, item] of entries) {
      const entry = normalizeProxy(item, id);
      if (entry && !byId.has(entry.id)) {
        byId.set(entry.id, entry);
        list.push(entry);
      }
    }
    return new PropProxies(list, byId);
  }

  const DEFAULT_PROXY_URLS = [
    '../assets/prop_proxies.json',
    '../../assets/prop_proxies.json',
    'assets/prop_proxies.json',
    'prop_proxies.json'
  ];

  /**
   * Loads the derived proxy geometry next to the catalogue. Missing files,
   * invalid JSON or malformed entries yield an empty set (never throws), so the
   * editor falls back to catalogue boxes and keeps working from file://.
   */
  async function loadPropProxies(urls, fetchImpl) {
    const candidates = Array.isArray(urls) && urls.length ? urls : DEFAULT_PROXY_URLS;
    const doFetch = fetchImpl || (typeof fetch === 'function' ? fetch : null);
    if (doFetch) {
      for (const url of candidates) {
        try {
          const response = await doFetch(url, { cache: 'no-store' });
          if (!response || !response.ok) continue;
          const data = await response.json();
          const proxies = parsePropProxies(data);
          if (proxies.size > 0) {
            proxies.source = url;
            return proxies;
          }
        } catch (err) {
          // Try the next candidate; an empty set is always available.
        }
      }
    }
    const empty = PropProxies.empty();
    empty.source = 'none';
    return empty;
  }

  return {
    PROP_CATEGORIES,
    PROP_FALLBACK_SIZE,
    PROP_FALLBACK_COLOR,
    BUILTIN_PROPS,
    DEFAULT_CATALOG_URLS,
    DEFAULT_PROXY_URLS,
    PropCatalog,
    PropProxies,
    parsePropCatalog,
    isPlaceableAsset,
    parsePropProxies,
    parseHexColor,
    toHexColor,
    loadPropCatalog,
    loadPropProxies
  };
});
