// props.test.mjs - Prop catalog parsing, search, fallbacks and the built-in sync check.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import props from '../js/props.js';

const { PropCatalog, PropProxies, parsePropCatalog, isPlaceableAsset, parseHexColor, loadPropCatalog, loadPropProxies } = props;
const here = path.dirname(fileURLToPath(import.meta.url));
const catalogPath = path.resolve(here, '../../assets/catalog.json');

test('the built-in catalog mirrors the placeable entries of assets/catalog.json', () => {
  const shared = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
  const expected = shared.assets.filter(isPlaceableAsset).map(entry => ({
    id: entry.id,
    name: entry.display_name || entry.name || entry.id,
    category: entry.category,
    size: entry.size,
    color: entry.color,
    model: entry.model === undefined ? null : entry.model,
    solid: entry.solid === true
  }));
  assert.deepEqual(props.BUILTIN_PROPS, expected);
});

test('the catalog parses entries and resolves colours', () => {
  const catalog = PropCatalog.fromJSON({ props: [{ id: 'core:plant', name: 'Plant', category: 'Decorative', size: [0.4, 1.0, 0.4], color: '#4f6b43' }] });
  assert.equal(catalog.size, 1);
  const entry = catalog.get('core:plant');
  assert.equal(entry.category, 'Decorative');
  assert.deepEqual(entry.size, [0.4, 1.0, 0.4]);
  assert.deepEqual(parseHexColor('#4f6b43'), [0x4f / 255, 0x6b / 255, 0x43 / 255]);
  assert.equal(parseHexColor('nope'), null);
});

test('malformed catalog entries fall back instead of breaking the editor', () => {
  const catalog = PropCatalog.fromJSON({ props: [
    { id: 'core:weird', size: [-1, 0, 0], color: 'not-a-colour', category: 'Nonsense' },
    { name: 'no id at all' },
    null
  ] });
  assert.equal(catalog.size, 1);
  const entry = catalog.get('core:weird');
  assert.deepEqual(entry.size, props.PROP_FALLBACK_SIZE);
  assert.deepEqual(entry.color, props.PROP_FALLBACK_COLOR);
  assert.equal(entry.category, 'Other');
});

test('unknown models resolve to a usable placeholder', () => {
  const catalog = PropCatalog.builtin();
  const missing = catalog.get('core:does_not_exist');
  assert.equal(missing.missing, true);
  assert.deepEqual(missing.size, props.PROP_FALLBACK_SIZE);
  assert.equal(catalog.has('core:does_not_exist'), false);
});

test('props can be searched and filtered by category', () => {
  const catalog = PropCatalog.builtin();
  assert.ok(catalog.categories().includes('Appliances'));
  assert.ok(catalog.categories().includes('Furniture'));
  const appliances = catalog.search('', 'Appliances');
  assert.ok(appliances.length >= 4);
  assert.ok(appliances.every(e => e.category === 'Appliances'));
  const sink = catalog.search('sink');
  assert.deepEqual(sink.map(e => e.id), ['core:sink']);
  assert.equal(catalog.search('', 'All').length, catalog.size);
  assert.equal(catalog.search('zzz').length, 0);
});

test('a catalogue whose file is missing falls back to the built-in list', async () => {
  const failing = async () => { throw new Error('offline'); };
  const catalog = await loadPropCatalog(['does-not-exist.json'], failing);
  assert.equal(catalog.source, 'builtin');
  assert.equal(catalog.size, props.BUILTIN_PROPS.length);
});

test('a fetched catalogue is used when it is available and valid', async () => {
  const payload = { props: [{ id: 'pack:custom_lamp', name: 'Custom Lamp', category: 'Decorative', size: [0.3, 1.2, 0.3], color: '#ffffff' }] };
  const fakeFetch = async () => ({ ok: true, json: async () => payload });
  const catalog = await loadPropCatalog(['props.json'], fakeFetch);
  assert.equal(catalog.source, 'props.json');
  assert.equal(catalog.size, 1);
  assert.ok(catalog.has('pack:custom_lamp'));
});

test('an empty fetched catalogue is rejected in favour of the built-in list', async () => {
  const fakeFetch = async () => ({ ok: true, json: async () => ({ props: [] }) });
  const catalog = await loadPropCatalog(['props.json'], fakeFetch);
  assert.equal(catalog.source, 'builtin');
});

test('solid props default to blocking, decorative ones do not', () => {
  const catalog = PropCatalog.builtin();
  assert.equal(catalog.get('core:stove').solid, true);
  assert.equal(catalog.get('core:rug').solid, false);
});

test('proxy geometry parses every part shape and resolves entries by id', () => {
  const proxies = PropProxies.fromJSON({
    format_version: 1,
    props: {
      'core:chair': {
        name: 'Chair',
        model: 'models/chair.glb',
        parts: [
          { shape: 'box', center: [0, 0.45, 0], size: [0.46, 0.05, 0.46], rotation: [0, 0, 0], color: '#a08a6a' },
          { shape: 'cylinder', axis: 'y', base: [0.2, 0, 0.2], radius: 0.02, height: 0.45, segments: 6, taper: 1.0, color: '#8a6f4d' },
          { shape: 'tube', start: [0, 0.4, 0], end: [0, 0.5, 0.1], radius: 0.012, color: '#8a6f4d' },
          { shape: 'plane', center: [0, 0.9, 0.25], size: [0.4, 0.4, 0.4], normal: 'y', color: '#7a6a55' }
        ]
      }
    }
  });

  assert.equal(proxies.size, 1);
  assert.equal(proxies.has('core:chair'), true);
  const proxy = proxies.get('core:chair');
  assert.equal(proxy.parts.length, 4);
  assert.deepEqual(proxy.parts[0].center, [0, 0.45, 0]);
  assert.deepEqual(proxy.parts[0].color, parseHexColor('#a08a6a'));
  assert.equal(proxy.parts[1].shape, 'cylinder');
  assert.equal(proxy.parts[1].segments, 6);
  assert.equal(proxy.parts[2].shape, 'tube');
  assert.equal(proxy.parts[3].normal, 'y');

  // Unknown ids behave like the catalogue: no proxy, no throw.
  assert.equal(proxies.has('core:does_not_exist'), false);
  assert.equal(proxies.get('core:does_not_exist'), null);
});

test('malformed proxy entries are dropped instead of breaking the editor', () => {
  const proxies = PropProxies.fromJSON({
    props: {
      'core:no_parts': { parts: [] },
      'core:bad_parts': { parts: [null, { shape: 'box' }, { shape: 'cylinder', base: [0, 0, 0], radius: -1, height: 1 }, { shape: 'rug' }] },
      'core:good': { parts: [{ shape: 'box', center: [0, 0, 0], size: [1, 1, 1], color: '#ffffff' }] },
      'core:partial': {
        parts: [
          { shape: 'box', center: [0, 0, 0], size: [1, 1, 1], color: 'not-a-colour' },
          { shape: 'plane', center: [0, 0, 0], size: [0, 1, 0], normal: 'y' }
        ]
      }
    }
  });

  assert.equal(proxies.size, 2);
  assert.equal(proxies.has('core:no_parts'), false, 'a proxy without parts falls back to the box');
  assert.equal(proxies.has('core:bad_parts'), false);
  assert.equal(proxies.get('core:good').parts.length, 1);
  const partial = proxies.get('core:partial');
  assert.equal(partial.parts.length, 1);
  assert.equal(partial.parts[0].shape, 'box', 'the zero-extent plane is dropped');
  assert.deepEqual(partial.parts[0].color, props.PROP_FALLBACK_COLOR, 'a bad colour falls back');

  // Junk payloads never throw and never produce usable proxies.
  assert.equal(PropProxies.fromJSON(null).size, 0);
  assert.equal(PropProxies.fromJSON([null, 42, 'nope']).size, 0);
});

test('a missing proxy file yields an empty set so the editor keeps working', async () => {
  const failing = async () => { throw new Error('offline'); };
  const proxies = await loadPropProxies(['does-not-exist.json'], failing);
  assert.equal(proxies.size, 0);
  assert.equal(proxies.has('core:chair'), false);
  assert.equal(proxies.get('core:chair'), null);
  assert.equal(proxies.source, 'none');
});

test('a fetched proxy file is used when valid; junk yields an empty set', async () => {
  const payload = {
    props: {
      'core:chair': {
        name: 'Chair',
        model: 'models/chair.glb',
        parts: [{ shape: 'box', center: [0, 0.45, 0], size: [0.46, 0.05, 0.46], rotation: [0, 0, 0], color: '#a08a6a' }]
      }
    }
  };
  const okFetch = async () => ({ ok: true, json: async () => payload });
  const proxies = await loadPropProxies(['prop_proxies.json'], okFetch);
  assert.equal(proxies.source, 'prop_proxies.json');
  assert.equal(proxies.size, 1);
  assert.equal(proxies.get('core:chair').parts[0].shape, 'box');

  const junk = async () => ({ ok: true, json: async () => ({ props: 'not an object' }) });
  assert.equal((await loadPropProxies(['prop_proxies.json'], junk)).size, 0);

  const notFound = async () => ({ ok: false, json: async () => ({}) });
  assert.equal((await loadPropProxies(['prop_proxies.json'], notFound)).size, 0);
});
