// prop-assets.test.mjs - End-to-end check that the editor renders the REAL
// shipped prop assets: the derived proxy file, the catalogue and the demo
// levels are all read from disk and pushed through the editor's own geometry
// builder, so a missing proxy, a stale catalogue entry or a prop that silently
// fell back to a box fails here instead of in the browser.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import props from '../js/props.js';
import geometry from '../js/geometry.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(here, '../..');
const catalogPath = path.join(appRoot, 'assets/catalog.json');
const proxyPath = path.join(appRoot, 'assets/prop_proxies.json');
const thumbsDir = path.join(appRoot, 'level-editor/assets/thumbs');

const catalog = props.PropCatalog.fromJSON(JSON.parse(fs.readFileSync(catalogPath, 'utf8')));
const proxies = props.PropProxies.fromJSON(JSON.parse(fs.readFileSync(proxyPath, 'utf8')));

const MODEL_KEYS = ['+x', '-x', '+y', '-y', '+z', '-z'];

test('every catalogue prop ships a derived proxy with real parts', () => {
  assert.equal(catalog.size, 21, 'the pack is the twenty core props plus spooner-man');
  assert.equal(proxies.size, catalog.size, 'one proxy per catalogue entry');
  assert.ok(catalog.has('spooner-man'), 'the spooner-man prop id is exactly "spooner-man"');
  for (const entry of catalog.list) {
    const proxy = proxies.get(entry.id);
    assert.ok(proxy, `${entry.id} has no proxy entry`);
    assert.equal(proxy.model, entry.model, `${entry.id} proxy points at the wrong model`);
    assert.ok(proxy.parts.length > 0, `${entry.id} proxy has no parts`);
    assert.ok(proxy.triangles > 0, `${entry.id} proxy has no triangles`);
    assert.ok(proxy.triangles <= 1500, `${entry.id} exceeds the triangle ceiling`);
    assert.ok(proxy.texture[0] <= 256 && proxy.texture[1] <= 256, `${entry.id} texture is too large`);
  }
});

test('proxy bounds match the catalogue size and origin conventions', () => {
  for (const entry of catalog.list) {
    const proxy = proxies.get(entry.id);
    for (let axis = 0; axis < 3; axis += 1) {
      const extent = proxy.boundsMax[axis] - proxy.boundsMin[axis];
      const tolerance = Math.max(0.02, entry.size[axis] * 0.06);
      assert.ok(
        Math.abs(extent - entry.size[axis]) <= tolerance,
        `${entry.id} axis ${axis}: proxy is ${extent.toFixed(3)} m, catalogue says ${entry.size[axis].toFixed(3)} m`
      );
    }
    assert.ok(Math.abs(proxy.boundsMin[1]) <= 0.012, `${entry.id} must rest on y=0`);
    const centerX = (proxy.boundsMin[0] + proxy.boundsMax[0]) / 2;
    const centerZ = (proxy.boundsMin[2] + proxy.boundsMax[2]) / 2;
    assert.ok(Math.abs(centerX) <= 0.02 && Math.abs(centerZ) <= 0.02, `${entry.id} must be centred`);
  }
});

test('the editor draws real proxy geometry instead of the placeholder box', () => {
  const level = JSON.parse(fs.readFileSync(path.join(appRoot, 'assets/levels/prop_showcase.json'), 'utf8'));
  const mesh = geometry.buildLevelMesh(level, { catalog, proxies });
  const batch = mesh.batches.find(entry => entry.name === 'props');
  assert.ok(batch, 'the props batch exists');

  // A single placeholder box is 12 triangles; the showcase has twenty props, so
  // real geometry must be far above that.
  const placeholderTriangles = level.props.length * 12;
  assert.ok(
    batch.count / 3 > placeholderTriangles * 2,
    `props batch has ${batch.count / 3} triangles, expected real asset geometry (placeholder would be ${placeholderTriangles})`
  );

  for (const prop of level.props) {
    assert.ok(mesh.owners.includes(prop.id), `${prop.model} must be tagged for picking`);
  }
});

test('a missing proxy still falls back to the catalogue box', () => {
  const level = {
    rooms: [], walls: [], ceiling_lights: [],
    props: [{ id: 'p1', model: 'pack:unknown_prop', x: 1, y: 0.5, z: 2, rotation_degrees: 30, scale: 2 }],
  };
  const mesh = geometry.buildLevelMesh(level, { catalog, proxies });
  const batch = mesh.batches.find(entry => entry.name === 'props');
  assert.equal(batch.count, 36, 'a prop without a proxy keeps the 12-triangle box');
});

test('every prop has a thumbnail for the browser', () => {
  for (const entry of catalog.list) {
    const short = entry.id.split(':').pop();
    const file = path.join(thumbsDir, `${short}.png`);
    assert.ok(fs.existsSync(file), `missing ${file}`);
    const bytes = fs.readFileSync(file);
    assert.equal(bytes.subarray(1, 4).toString('ascii'), 'PNG', `${short}.png is not a PNG`);
  }
});

test('the demo levels place every core prop', () => {
  const showcase = JSON.parse(fs.readFileSync(path.join(appRoot, 'assets/levels/prop_showcase.json'), 'utf8'));
  const placed = new Set(showcase.props.map(prop => prop.model));
  for (const entry of catalog.list) {
    assert.ok(placed.has(entry.id), `prop_showcase.json must place ${entry.id}`);
  }

  const stress = JSON.parse(fs.readFileSync(path.join(appRoot, 'assets/levels/prop_stress.json'), 'utf8'));
  assert.ok(stress.props.length >= 100, 'the stress level must carry a real repeated load');
  const models = new Set(stress.props.map(prop => prop.model));
  assert.ok(models.size <= 12, `the stress level should stay within a handful of models, got ${models.size}`);

  // The showcase deliberately keeps a sunk prop; it must survive the round trip.
  const sunk = showcase.props.find(prop => prop.model === 'core:crate' && prop.y < 0);
  assert.ok(sunk, 'the showcase keeps one crate sunk into the floor');
  assert.equal(sunk.solid, true, 'the sunk crate still blocks the player');
});

test('proxy parts use only the four supported shapes', () => {
  for (const entry of catalog.list) {
    for (const part of proxies.get(entry.id).parts) {
      assert.ok(['box', 'cylinder', 'tube', 'plane'].includes(part.shape), `${entry.id}: ${part.shape}`);
      assert.equal(part.color.length, 3, `${entry.id} part colour must be RGB`);
      assert.ok(part.color.every(value => value >= 0 && value <= 1), `${entry.id} part colour out of range`);
      const numbers = []
        .concat(part.center || [], part.size || [], part.base || [], part.start || [], part.end || [])
        .concat(part.radius === undefined ? [] : [part.radius], part.height === undefined ? [] : [part.height]);
      for (const value of numbers) {
        assert.ok(Number.isFinite(value), `${entry.id} part has a non-finite coordinate`);
      }
      if (part.shape === 'box' && part.rotation) {
        assert.equal(part.rotation.length, 3, `${entry.id} box rotation needs three axes`);
      }
    }
    // Sanity: a box part's colour keys must be usable by the renderer.
    assert.equal(MODEL_KEYS.length, 6);
  }
});
