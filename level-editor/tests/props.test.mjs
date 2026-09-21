// props.test.mjs - Prop catalog parsing, search, fallbacks and the built-in sync check.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import props from '../js/props.js';

const { PropCatalog, parsePropCatalog, parseHexColor, loadPropCatalog } = props;
const here = path.dirname(fileURLToPath(import.meta.url));
const catalogPath = path.resolve(here, '../../assets/props/props.json');

test('the built-in catalog mirrors assets/props/props.json', () => {
  const shared = JSON.parse(fs.readFileSync(catalogPath, 'utf8'));
  const expected = shared.props.map(entry => ({
    id: entry.id,
    name: entry.name,
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
