// ops.js - DOM-free level editing operations.
//
// Every mutation the editor performs lives here so it can be unit tested without a
// browser and so the 2D canvas, the 3D viewport and the inspector all share exactly
// one implementation of "what does this tool do". UI modules (editor.js, app.js,
// properties.js) only translate input into these calls and then refresh their views.
//
// Design rules:
//   * Intentional overlap/clipping is never "corrected": a prop may sit inside a
//     wall or below the floor, rooms and walls may intersect.
//   * Openings are owned by their wall and stored in wall-local coordinates, so
//     moving/resizing a wall moves its doors and windows with it.
//   * Operations that cannot be performed return null instead of throwing.

(function (root, factory) {
  const deps = (typeof module !== 'undefined' && module.exports)
    ? { model: require('./model.js'), geometry: require('./geometry.js') }
    : { model: root, geometry: root.LiminalGeometry };
  const api = factory(deps.model, deps.geometry);
  if (typeof module !== 'undefined' && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.LiminalOps = api;
    root.Ops = api;
  }
})(typeof window !== 'undefined' ? window : null, function (model, geometry) {
  'use strict';

  const { Room, Wall, WallOpening, CeilingLight, Prop, FloorPatch } = model;

  // --------------------------------------------------------------- creation

  function createRoom(level, data) {
    const room = new Room({
      x: data.x,
      z: data.z,
      width: Math.max(0.5, Math.abs(data.width || 10)),
      depth: Math.max(0.5, Math.abs(data.depth || 10)),
      height: Math.max(1, data.height || level.getCeilingHeight() || 3.5)
    });
    level.rooms.push(room);
    return room;
  }

  function createWall(level, data) {
    const wall = new Wall({
      x: data.x,
      z: data.z,
      width: Math.max(0.05, Math.abs(data.width || 2)),
      depth: Math.max(0.05, Math.abs(data.depth || 0.35)),
      y: data.y || 0,
      height: data.height === undefined ? null : data.height
    });
    level.walls.push(wall);
    return wall;
  }

  function addLight(level, data) {
    const light = new CeilingLight({
      x: data.x || 0,
      z: data.z || 0,
      fixture: (data && data.fixture) || 'core:fluorescent_panel_01',
      rotation_degrees: (data && data.rotation_degrees) || 0,
      brightness: data && data.brightness !== undefined ? data.brightness : 1.0
    });
    level.ceiling_lights.push(light);
    return light;
  }

  function addProp(level, data, catalog) {
    if (!data || !data.model) return null;
    const entry = catalog && typeof catalog.get === 'function' ? catalog.get(data.model) : null;
    const prop = new Prop({
      model: data.model,
      x: data.x || 0,
      y: data.y || 0,
      z: data.z || 0,
      rotation_degrees: data.rotation_degrees || 0,
      scale: data.scale === undefined ? 1 : data.scale,
      size: data.size || null,
      solid: data.solid === undefined ? !!(entry && entry.solid) : data.solid === true
    });
    level.props.push(prop);
    return prop;
  }

  function addFloorPatch(level, data) {
    const patch = new FloorPatch({
      x: data.x || 0,
      z: data.z || 0,
      width: Math.max(0.25, Math.abs(data.width || 2)),
      depth: Math.max(0.25, Math.abs(data.depth || 2)),
      material: (data && data.material) || 'core:carpet_damp_01'
    });
    level.floor_patches.push(patch);
    return patch;
  }

  function setSpawn(level, data) {
    level.spawn.x = Number(data.x) || 0;
    level.spawn.z = Number(data.z) || 0;
    if (data.yaw_degrees !== undefined) level.spawn.yaw_degrees = Number(data.yaw_degrees) || 0;
    return level.spawn;
  }

  /**
   * Adds the four perimeter walls of a room rectangle, skipping edges that already
   * have a matching wall. Explicit and repeatable - never destructive.
   */
  function addWallsAroundRect(level, rect, options) {
    const opts = options || {};
    const thickness = Math.max(0.05, opts.thickness || 0.35);
    const height = opts.height === undefined ? level.getCeilingHeight() : opts.height;
    const edges = [
      { x: rect.x - thickness, z: rect.z - thickness, width: rect.width + thickness * 2, depth: thickness },
      { x: rect.x - thickness, z: rect.z + rect.depth, width: rect.width + thickness * 2, depth: thickness },
      { x: rect.x - thickness, z: rect.z, width: thickness, depth: rect.depth },
      { x: rect.x + rect.width, z: rect.z, width: thickness, depth: rect.depth }
    ];
    const created = [];
    for (const edge of edges) {
      const exists = level.walls.some(w =>
        Math.abs(w.x - edge.x) < 0.01 && Math.abs(w.z - edge.z) < 0.01 &&
        Math.abs(Math.abs(w.width) - edge.width) < 0.01 && Math.abs(Math.abs(w.depth) - edge.depth) < 0.01
      );
      if (!exists) created.push(createWall(level, { ...edge, height }));
    }
    return created;
  }

  // --------------------------------------------------------- wall openings

  const OPENING_KINDS = geometry.OPENING_KINDS;

  function defaultOpening(kind) {
    const preset = OPENING_KINDS[kind] || OPENING_KINDS.door;
    return { kind, width: preset.width, height: preset.height, sill: preset.sill };
  }

  /**
   * Adds an opening to a wall, clamped into the wall and cleared of invalid values.
   * Offsets are measured along the wall's length axis (see geometry.js).
   */
  function addOpening(wall, data) {
    const length = geometry.wallLength(wall);
    const height = geometry.wallResolvedHeight(wall, 3.5);
    if (!(length > 0.1)) return null;
    const kind = OPENING_KINDS[data.kind] ? data.kind : 'door';
    const preset = defaultOpening(kind);
    const width = Math.max(0.2, Math.min(length, Number(data.width) || preset.width));
    const openingHeight = Math.max(0.2, Number(data.height) || preset.height);
    const sill = Math.max(0, Number(data.sill) || preset.sill);
    const offset = Math.max(0, Math.min(length - width, Number(data.offset) || 0));

    const opening = new WallOpening({
      kind,
      offset,
      width,
      height: Math.min(openingHeight, Math.max(0.2, height - sill)),
      sill
    });
    wall.openings.push(opening);
    return opening;
  }

  /** Places an opening on whichever wall is nearest to a world point. */
  function addOpeningAtPoint(level, wall, world, kind, options) {
    if (!wall) return null;
    const opts = options || {};
    const preset = defaultOpening(kind);
    const length = geometry.wallLength(wall);
    const clicked = geometry.wallProjectOffset(wall, world);
    const width = Math.max(0.2, Math.min(length, opts.width || preset.width));
    const centre = opts.align === 'start' ? clicked : clicked - width / 2;
    const offset = Math.max(0, Math.min(length - width, centre));
    return addOpening(wall, {
      kind,
      offset,
      width,
      height: opts.height !== undefined ? opts.height : preset.height,
      sill: opts.sill !== undefined ? opts.sill : preset.sill
    });
  }

  function findOpening(level, openingId) {
    for (const wall of level.walls) {
      for (const opening of wall.openings) {
        if (opening.id === openingId) return { wall, opening };
      }
    }
    return null;
  }

  function removeOpening(level, openingId) {
    for (const wall of level.walls) {
      const idx = wall.openings.findIndex(o => o.id === openingId);
      if (idx >= 0) {
        wall.openings.splice(idx, 1);
        return true;
      }
    }
    return false;
  }

  // ---------------------------------------------------------------- move

  const MOVABLE = ['rooms', 'walls', 'ceiling_lights', 'props', 'floor_patches'];

  function findObject(level, id) {
    for (const key of MOVABLE) {
      const found = (level[key] || []).find(o => o.id === id);
      if (found) return { key, object: found };
    }
    if (id === 'spawn') return { key: 'spawn', object: level.spawn };
    return null;
  }

  function moveObjects(level, ids, dx, dz) {
    let moved = 0;
    for (const id of ids) {
      const found = findObject(level, id);
      if (!found || found.key === 'spawn' && !found.object) continue;
      const obj = found.object;
      obj.x = Number((obj.x + dx).toFixed(4));
      obj.z = Number((obj.z + dz).toFixed(4));
      moved++;
    }
    return moved;
  }

  /** Moves a single object to an absolute position (used by the inspector). */
  function moveObjectTo(level, id, x, z) {
    const found = findObject(level, id);
    if (!found) return false;
    found.object.x = Number(x) || 0;
    found.object.z = Number(z) || 0;
    return true;
  }

  // ---------------------------------------------------------------- resize

  const MIN_SIZE = 0.05;

  /** XZ rectangle (x, z, width, depth) of a prop, rotation-aware. */
  function propRect2D(prop, catalog) {
    const b = geometry.propBounds(prop, catalog);
    return { x: b.minX, z: b.minZ, width: b.maxX - b.minX, depth: b.maxZ - b.minZ, kind: 'prop' };
  }

  /** 2D bounds of any object, including opening parts. */
  function objectBounds2D(level, id, catalog) {
    const wall = level.walls.find(w => w.id === id);
    if (wall) {
      const min = geometry.wallMinCorner(wall);
      return { x: min.x, z: min.z, width: Math.abs(wall.width), depth: Math.abs(wall.depth), kind: 'wall' };
    }
    const room = level.rooms.find(r => r.id === id);
    if (room) {
      return {
        x: Math.min(room.x, room.x + room.width),
        z: Math.min(room.z, room.z + room.depth),
        width: Math.abs(room.width),
        depth: Math.abs(room.depth),
        kind: 'room'
      };
    }
    const patch = level.floor_patches.find(p => p.id === id);
    if (patch) {
      return { x: patch.x, z: patch.z, width: Math.abs(patch.width), depth: Math.abs(patch.depth), kind: 'patch' };
    }
    const prop = level.props.find(p => p.id === id);
    if (prop) return propRect2D(prop, catalog);
    const light = level.ceiling_lights.find(l => l.id === id);
    if (light) {
      const turned = Math.round(light.rotation_degrees / 90) % 2 !== 0;
      const halfW = turned ? 0.3 : 0.6;
      const halfD = turned ? 0.6 : 0.3;
      return { x: light.x - halfW, z: light.z - halfD, width: halfW * 2, depth: halfD * 2, kind: 'light' };
    }
    const openingRef = findOpening(level, id);
    if (openingRef) {
      const rect = openingBounds2D(openingRef.wall, openingRef.opening);
      return { ...rect, kind: 'opening' };
    }
    if (id === 'spawn' && level.spawn) {
      return { x: level.spawn.x - 0.45, z: level.spawn.z - 0.45, width: 0.9, depth: 0.9, kind: 'spawn' };
    }
    return null;
  }

  /** XZ rectangle occupied by an opening (its span across the wall's thickness). */
  function openingBounds2D(wall, opening) {
    const thickness = geometry.wallThickness(wall);
    const a = geometry.wallLocalToWorld(wall, opening.offset, 0);
    if (geometry.wallAxis(wall) === 'x') {
      return { x: a.x, z: a.z, width: opening.width, depth: thickness };
    }
    return { x: a.x, z: a.z, width: thickness, depth: opening.width };
  }

  /** Keeps every opening inside its wall after the wall changes size. */
  function clampWallOpenings(wall) {
    const length = geometry.wallLength(wall);
    for (const opening of wall.openings) {
      opening.width = Math.min(length, Math.max(0.2, Number(opening.width) || 0.2));
      opening.offset = Math.max(0, Math.min(Math.max(0, length - opening.width), Number(opening.offset) || 0));
    }
  }

  const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'];

  /**
   * Resizes an object by dragging one of its 8 handles to a world point.
   * Rooms/walls/patches resize their rectangle; lights/spawn/props only move.
   * Openings resize along their wall only (offset/width for e/w handles, sill/height
   * for n/s handles on the relevant axis).
   */
  function resizeObject(level, id, handle, world, initial) {
    const openingRef = findOpening(level, id);
    if (openingRef) return resizeOpening(openingRef.wall, openingRef.opening, handle, world, initial);

    const target = findObject(level, id);
    if (!target) return false;
    const obj = target.object;
    if (target.key === 'ceiling_lights' || target.key === 'spawn' || target.key === 'props') {
      const b = objectBounds2D(level, id);
      if (!b) return false;
      obj.x = Number((world.x).toFixed(4));
      obj.z = Number((world.z).toFixed(4));
      return true;
    }

    const bounds = initial || objectBounds2D(level, id);
    if (!bounds) return false;
    let x1 = bounds.x;
    let x2 = bounds.x + bounds.width;
    let z1 = bounds.z;
    let z2 = bounds.z + bounds.depth;

    if (handle.includes('w')) x1 = world.x;
    if (handle.includes('e')) x2 = world.x;
    if (handle.includes('n')) z1 = world.z;
    if (handle.includes('s')) z2 = world.z;

    const minX = Math.min(x1, x2);
    const maxX = Math.max(x1, x2);
    const minZ = Math.min(z1, z2);
    const maxZ = Math.max(z1, z2);

    obj.x = Number(minX.toFixed(4));
    obj.z = Number(minZ.toFixed(4));
    obj.width = Number(Math.max(MIN_SIZE, maxX - minX).toFixed(4));
    obj.depth = Number(Math.max(MIN_SIZE, maxZ - minZ).toFixed(4));
    if (target.key === 'walls') clampWallOpenings(obj);
    return true;
  }

  function resizeOpening(wall, opening, handle, world, initial) {
    const length = geometry.wallLength(wall);
    const u = geometry.wallProjectOffset(wall, world);

    let offset = initial ? initial.offset : opening.offset;
    let width = initial ? initial.width : opening.width;

    if (handle.includes('w')) {
      const end = offset + width;
      offset = Math.max(0, Math.min(end - 0.2, u));
      width = end - offset;
    } else if (handle.includes('e')) {
      width = Math.max(0.2, Math.min(length - offset, u - offset));
    }

    opening.offset = Number(Math.max(0, Math.min(length - 0.2, offset)).toFixed(4));
    opening.width = Number(Math.max(0.2, Math.min(length - opening.offset, width)).toFixed(4));
    return true;
  }

  // -------------------------------------------------------------- hit testing

  /**
   * Finds the object under a world point (2D top-down).
   * Priority: spawn, props, lights, openings, walls, room edges, room interior.
   */
  function hitTest2D(level, world, tolerance, catalog) {
    const tol = Math.max(0.02, tolerance || 0.15);

    if (level.spawn && Math.hypot(world.x - level.spawn.x, world.z - level.spawn.z) <= Math.max(0.45, tol)) {
      return { id: 'spawn', kind: 'spawn' };
    }

    for (let i = level.props.length - 1; i >= 0; i--) {
      const prop = level.props[i];
      if (pointInRect(world, propRect2D(prop, catalog), tol)) return { id: prop.id, kind: 'prop' };
    }

    for (let i = level.ceiling_lights.length - 1; i >= 0; i--) {
      const light = level.ceiling_lights[i];
      const turned = Math.round(light.rotation_degrees / 90) % 2 !== 0;
      const halfW = (turned ? 0.3 : 0.6) + tol;
      const halfD = (turned ? 0.6 : 0.3) + tol;
      if (Math.abs(world.x - light.x) <= halfW && Math.abs(world.z - light.z) <= halfD) {
        return { id: light.id, kind: 'light' };
      }
    }

    // Openings sit on top of their wall so clicking a doorway picks the doorway.
    for (let i = level.walls.length - 1; i >= 0; i--) {
      const wall = level.walls[i];
      for (const opening of wall.openings) {
        if (pointInRect(world, openingBounds2D(wall, opening), tol * 0.5)) {
          return { id: opening.id, kind: 'opening', wallId: wall.id, openingId: opening.id };
        }
      }
    }

    for (let i = level.walls.length - 1; i >= 0; i--) {
      const wall = level.walls[i];
      if (geometry.wallDistanceToPoint(wall, world) <= tol) return { id: wall.id, kind: 'wall' };
    }

    for (let i = level.rooms.length - 1; i >= 0; i--) {
      const room = level.rooms[i];
      const rect = {
        x: Math.min(room.x, room.x + room.width),
        z: Math.min(room.z, room.z + room.depth),
        width: Math.abs(room.width),
        depth: Math.abs(room.depth)
      };
      const nearEdge = (
        Math.abs(world.x - rect.x) <= tol || Math.abs(world.x - (rect.x + rect.width)) <= tol ||
        Math.abs(world.z - rect.z) <= tol || Math.abs(world.z - (rect.z + rect.depth)) <= tol
      ) && pointInRect(world, rect, 0);
      if (nearEdge) return { id: room.id, kind: 'room' };
    }

    for (let i = level.rooms.length - 1; i >= 0; i--) {
      const room = level.rooms[i];
      const rect = {
        x: Math.min(room.x, room.x + room.width),
        z: Math.min(room.z, room.z + room.depth),
        width: Math.abs(room.width),
        depth: Math.abs(room.depth)
      };
      if (pointInRect(world, rect, 0)) return { id: room.id, kind: 'room', interior: true };
    }

    return null;
  }

  /** Nearest wall within the tolerance, for the door/window tools. */
  function wallAtPoint(level, world, tolerance) {
    const tol = Math.max(0.05, tolerance || 0.3);
    let best = null;
    for (const wall of level.walls) {
      const distance = geometry.wallDistanceToPoint(wall, world);
      if (distance <= tol && (!best || distance < best.distance)) best = { wall, distance };
    }
    return best ? best.wall : null;
  }

  function pointInRect(point, rect, tolerance) {
    const tol = tolerance || 0;
    return point.x >= rect.x - tol && point.x <= rect.x + rect.width + tol &&
      point.z >= rect.z - tol && point.z <= rect.z + rect.depth + tol;
  }

  /** All object ids fully or partially inside an XZ rectangle (marquee selection). */
  function objectsInRect(level, rect, catalog) {
    const ids = [];
    const overlaps = (b) => b && b.x <= rect.x + rect.width && b.x + b.width >= rect.x &&
      b.z <= rect.z + rect.depth && b.z + b.depth >= rect.z;
    if (overlaps(objectBounds2D(level, 'spawn'))) ids.push('spawn');
    for (const room of level.rooms) if (overlaps(objectBounds2D(level, room.id))) ids.push(room.id);
    for (const wall of level.walls) if (overlaps(objectBounds2D(level, wall.id))) ids.push(wall.id);
    for (const light of level.ceiling_lights) if (overlaps(objectBounds2D(level, light.id))) ids.push(light.id);
    for (const prop of level.props) if (overlaps(objectBounds2D(level, prop.id, catalog))) ids.push(prop.id);
    return ids;
  }

  // --------------------------------------------------------- delete / duplicate

  /**
   * Deletes objects by id. The player spawn and the last room cannot be deleted
   * (the game requires both): the spawn is moved to the origin instead and the
   * final room is kept.
   */
  function deleteObjects(level, ids) {
    const set = new Set(ids);
    let deleted = 0;

    level.walls = level.walls.filter((w) => {
      if (set.has(w.id)) { deleted++; return false; }
      return true;
    });
    level.ceiling_lights = level.ceiling_lights.filter((l) => {
      if (set.has(l.id)) { deleted++; return false; }
      return true;
    });
    level.props = level.props.filter((p) => {
      if (set.has(p.id)) { deleted++; return false; }
      return true;
    });
    level.floor_patches = level.floor_patches.filter((p) => {
      if (set.has(p.id)) { deleted++; return false; }
      return true;
    });
    for (const wall of level.walls) {
      wall.openings = wall.openings.filter((o) => {
        if (set.has(o.id)) { deleted++; return false; }
        return true;
      });
    }

    const remainingRooms = level.rooms.filter((r) => {
      if (set.has(r.id)) { deleted++; return false; }
      return true;
    });
    level.rooms = remainingRooms.length > 0 ? remainingRooms : level.rooms.slice(0, 1);

    let spawnReset = false;
    if (set.has('spawn')) {
      spawnReset = true;
      level.spawn.x = 0;
      level.spawn.z = 0;
    }
    return { deleted, spawnReset };
  }

  /** Duplicates objects, offsetting the copies, and returns the new ids. */
  function duplicateObjects(level, ids, offset) {
    const dx = offset && offset.dx !== undefined ? offset.dx : 0.5;
    const dz = offset && offset.dz !== undefined ? offset.dz : 0.5;
    const newIds = [];

    for (const id of ids) {
      const room = level.rooms.find(r => r.id === id);
      if (room) {
        const copy = room.duplicate();
        copy.x += dx; copy.z += dz;
        level.rooms.push(copy);
        newIds.push(copy.id);
        continue;
      }
      const wall = level.walls.find(w => w.id === id);
      if (wall) {
        const copy = wall.duplicate();
        copy.x += dx; copy.z += dz;
        level.walls.push(copy);
        newIds.push(copy.id);
        continue;
      }
      const light = level.ceiling_lights.find(l => l.id === id);
      if (light) {
        const copy = light.duplicate();
        copy.x += dx; copy.z += dz;
        level.ceiling_lights.push(copy);
        newIds.push(copy.id);
        continue;
      }
      const prop = level.props.find(p => p.id === id);
      if (prop) {
        const copy = prop.duplicate();
        copy.x += dx; copy.z += dz;
        level.props.push(copy);
        newIds.push(copy.id);
        continue;
      }
      const patch = level.floor_patches.find(p => p.id === id);
      if (patch) {
        const copy = patch.duplicate();
        copy.x += dx; copy.z += dz;
        level.floor_patches.push(copy);
        newIds.push(copy.id);
        continue;
      }
      const openingRef = findOpening(level, id);
      if (openingRef) {
        const copy = openingRef.opening.duplicate();
        const length = geometry.wallLength(openingRef.wall);
        copy.offset = Math.max(0, Math.min(Math.max(0, length - copy.width), copy.offset + dx));
        openingRef.wall.openings.push(copy);
        newIds.push(copy.id);
      }
    }
    return newIds;
  }

  // ------------------------------------------------------------- serialization

  function serializeLevel(level, pretty) {
    return JSON.stringify(level.toJSON(), null, pretty === false ? 0 : 2);
  }

  function deserializeLevel(json, LevelClass) {
    const data = typeof json === 'string' ? JSON.parse(json) : json;
    const Ctor = LevelClass || model.Level;
    return new Ctor(data);
  }

  return {
    OPENING_KINDS,
    HANDLES,
    MIN_SIZE,
    createRoom,
    createWall,
    addLight,
    addProp,
    addFloorPatch,
    setSpawn,
    addWallsAroundRect,
    defaultOpening,
    addOpening,
    addOpeningAtPoint,
    findOpening,
    removeOpening,
    findObject,
    moveObjects,
    moveObjectTo,
    objectBounds2D,
    propRect2D,
    openingBounds2D,
    clampWallOpenings,
    resizeObject,
    resizeOpening,
    hitTest2D,
    wallAtPoint,
    pointInRect,
    objectsInRect,
    deleteObjects,
    duplicateObjects,
    serializeLevel,
    deserializeLevel
  };
});
