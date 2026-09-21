// model.js - Data structures, core materials, and validation for Liminal 2D Level Editor

// Highest ceiling-light intensity the game uses before clamping (mirrors
// src/lighting.rs::MAX_LIGHT_INTENSITY). Higher values still load; the game
// simply saturates them, so the editor warns instead of rejecting.
const LIGHT_INTENSITY_MAX = 8.0;

const CORE_MATERIALS = {
  'core:wallpaper_yellow_01': {
    id: 'core:wallpaper_yellow_01',
    name: 'Yellow Wallpaper',
    category: 'wall',
    color: '#b89c56',
    border: '#8e7436'
  },
  'core:wallpaper_stained_01': {
    id: 'core:wallpaper_stained_01',
    name: 'Stained Wallpaper',
    category: 'wall',
    color: '#8c733e',
    border: '#604c24'
  },
  'core:carpet_beige_01': {
    id: 'core:carpet_beige_01',
    name: 'Beige Carpet',
    category: 'floor',
    color: '#7a6f58',
    border: '#574d3a'
  },
  'core:carpet_damp_01': {
    id: 'core:carpet_damp_01',
    name: 'Damp Carpet',
    category: 'floor',
    color: '#474032',
    border: '#2c271e'
  },
  'core:ceiling_panel_01': {
    id: 'core:ceiling_panel_01',
    name: 'Ceiling Panel',
    category: 'ceiling',
    color: '#d5d5ce',
    border: '#9a9a92'
  },
  'core:ceiling_stained_01': {
    id: 'core:ceiling_stained_01',
    name: 'Stained Ceiling',
    category: 'ceiling',
    color: '#c3b49c',
    border: '#8d7d63'
  },
  'core:fluorescent_panel_01': {
    id: 'core:fluorescent_panel_01',
    name: 'Fluorescent Panel Light',
    category: 'fixture',
    color: '#fffde8',
    border: '#e6c84b'
  }
};

// Generates procedural thumbnails matching the game's built-in textures
const CORE_THUMBNAILS = {};
function initCoreThumbnails() {
  const size = 32;
  const canvas = document.createElement('canvas');
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext('2d');

  // 1. Yellow Wallpaper
  ctx.fillStyle = '#c4a65c';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#a68a44';
  for (let x = 0; x < size; x += 4) {
    ctx.fillRect(x, 0, 1.5, size);
  }
  CORE_THUMBNAILS['core:wallpaper_yellow_01'] = canvas.toDataURL();

  // 2. Stained Wallpaper
  ctx.fillStyle = '#8f743c';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#59441f';
  for (let x = 0; x < size; x += 4) {
    ctx.fillRect(x, 0, 1.5, size);
  }
  ctx.fillStyle = 'rgba(40, 28, 10, 0.45)';
  ctx.beginPath();
  ctx.arc(16, 20, 10, 0, Math.PI * 2);
  ctx.fill();
  CORE_THUMBNAILS['core:wallpaper_stained_01'] = canvas.toDataURL();

  // 3. Beige Carpet
  ctx.fillStyle = '#7a6f58';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#615743';
  for (let i = 0; i < 120; i++) {
    const rx = (i * 17) % size;
    const ry = (i * 23) % size;
    ctx.fillRect(rx, ry, 1, 1);
  }
  CORE_THUMBNAILS['core:carpet_beige_01'] = canvas.toDataURL();

  // 4. Damp Carpet
  ctx.fillStyle = '#484032';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#312a1f';
  for (let i = 0; i < 150; i++) {
    const rx = (i * 13) % size;
    const ry = (i * 29) % size;
    ctx.fillRect(rx, ry, 1, 1);
  }
  ctx.fillStyle = 'rgba(20, 20, 15, 0.5)';
  ctx.beginPath();
  ctx.ellipse(16, 16, 12, 8, Math.PI / 4, 0, Math.PI * 2);
  ctx.fill();
  CORE_THUMBNAILS['core:carpet_damp_01'] = canvas.toDataURL();

  // 5. Ceiling Panel
  ctx.fillStyle = '#d5d5ce';
  ctx.fillRect(0, 0, size, size);
  ctx.strokeStyle = '#9a9a92';
  ctx.lineWidth = 1;
  ctx.strokeRect(1, 1, size - 2, size - 2);
  ctx.strokeRect(size / 2, 0, 0, size);
  ctx.strokeRect(0, size / 2, size, 0);
  CORE_THUMBNAILS['core:ceiling_panel_01'] = canvas.toDataURL();

  // 6. Water-Damaged Ceiling: the same grid with one panel soaked through
  ctx.fillStyle = '#d3d2ca';
  ctx.fillRect(0, 0, size, size);
  ctx.strokeStyle = '#9a9a92';
  ctx.lineWidth = 1;
  ctx.strokeRect(1, 1, size - 2, size - 2);
  ctx.strokeRect(size / 2, 0, 0, size);
  ctx.strokeRect(0, size / 2, size, 0);
  ctx.fillStyle = 'rgba(120, 88, 52, 0.55)';
  ctx.beginPath();
  ctx.ellipse(size * 0.75, size * 0.75, size * 0.20, size * 0.16, 0.5, 0, Math.PI * 2);
  ctx.fill();
  CORE_THUMBNAILS['core:ceiling_stained_01'] = canvas.toDataURL();

  // 7. Fluorescent Panel Light
  ctx.fillStyle = '#333333';
  ctx.fillRect(0, 0, size, size);
  ctx.fillStyle = '#fffde8';
  ctx.fillRect(4, 8, size - 8, size - 16);
  ctx.strokeStyle = '#e6c84b';
  ctx.lineWidth = 1;
  ctx.strokeRect(4, 8, size - 8, size - 16);
  CORE_THUMBNAILS['core:fluorescent_panel_01'] = canvas.toDataURL();
}

let nextIdCounter = 1;
function generateUniqueId(prefix) {
  return `${prefix}_${Date.now().toString(36)}_${(nextIdCounter++).toString(36)}`;
}

/** Most frequent value in a list (first value wins ties), or null when empty. */
function majorityValue(values) {
  const counts = new Map();
  let best = null;
  let bestCount = 0;
  for (const value of values) {
    const count = (counts.get(value) || 0) + 1;
    counts.set(value, count);
    if (count > bestCount) {
      best = value;
      bestCount = count;
    }
  }
  return best;
}

/**
 * A rectangular cutout through a wall's thickness: a doorway, window, passage or vent.
 * Mirrors `WallOpeningDef` in src/level.rs. `offset` is measured in metres along the
 * wall's length axis from the wall's minimum corner; `sill` is the bottom edge height
 * above the wall base (0 for a walk-through doorway).
 */
class WallOpening {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('opening');
    this.kind = data.kind || 'door';
    this.offset = Number(data.offset ?? 0);
    this.width = Number(data.width ?? 1.0);
    this.height = Number(data.height ?? 2.1);
    this.sill = Number(data.sill ?? 0);
  }

  get end() {
    return this.offset + this.width;
  }

  /** Deep copy for history snapshots - keeps the id so selection stays stable. */
  clone() {
    return new WallOpening({
      id: this.id,
      kind: this.kind,
      offset: this.offset,
      width: this.width,
      height: this.height,
      sill: this.sill
    });
  }

  /** Copy with a fresh id, for duplicating objects in the level. */
  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('opening');
    return copy;
  }

  toJSON() {
    const obj = {
      kind: this.kind,
      offset: Number(this.offset.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      height: Number(this.height.toFixed(3))
    };
    if (this.sill) obj.sill = Number(this.sill.toFixed(3));
    return obj;
  }
}

class Wall {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('wall');
    this.x = Number(data.x ?? 0);
    this.y = Number(data.y ?? 0);
    this.z = Number(data.z ?? 0);
    this.width = Number(data.width ?? 1);
    this.depth = Number(data.depth ?? 0.35);
    // height: null/undefined represents default room ceiling height
    this.height = data.height !== undefined && data.height !== null ? Number(data.height) : null;
    this.faces = data.faces ? { ...data.faces } : {};
    // A single material applied to most faces is stored in the format's `faces`
    // map (the game's serialized field) and surfaced as one simple dropdown.
    const faceValues = Object.entries(this.faces)
      .filter(([key, value]) => value && ['north', 'south', 'east', 'west'].includes(key))
      .map(([, value]) => value);
    this.material = data.material || majorityValue(faceValues) || null;
    this.openings = (data.openings || []).map(o => new WallOpening(o));
  }

  isFullHeight(ceilingHeight = 3.5) {
    return this.y === 0 && (this.height === null || this.height >= ceilingHeight);
  }

  isRaised() {
    return this.y > 0.001;
  }

  isHalfHeight(ceilingHeight = 3.5) {
    return this.y === 0 && this.height !== null && this.height < ceilingHeight;
  }

  getResolvedHeight(defaultCeiling = 3.5) {
    return this.height !== null && this.height !== undefined ? this.height : defaultCeiling;
  }

  clone() {
    return new Wall({
      id: this.id,
      x: this.x,
      y: this.y,
      z: this.z,
      width: this.width,
      depth: this.depth,
      height: this.height,
      faces: { ...this.faces },
      material: this.material,
      openings: this.openings.map(o => o.clone())
    });
  }

  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('wall');
    for (const opening of copy.openings) opening.id = generateUniqueId('opening');
    return copy;
  }

  toJSON() {
    const obj = {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3))
    };
    if (this.y !== 0) {
      obj.y = Number(this.y.toFixed(3));
    }
    if (this.height !== null && this.height !== undefined) {
      obj.height = Number(this.height.toFixed(3));
    }
    const faces = {};
    if (this.faces) {
      for (const [k, v] of Object.entries(this.faces)) {
        if (v && v.trim()) faces[k] = v.trim();
      }
    }
    // The simple "wall material" control is serialized through the format's
    // per-face material map so the choice is never silently discarded.
    if (this.material) {
      for (const face of ['north', 'south', 'east', 'west']) {
        if (!faces[face]) faces[face] = this.material;
      }
    }
    if (Object.keys(faces).length > 0) {
      obj.faces = faces;
    }
    if (this.openings.length > 0) {
      obj.openings = this.openings.map(o => o.toJSON());
    }
    return obj;
  }
}

/**
 * A placed prop / furniture / appliance instance. Mirrors `PropDef` in src/level.rs:
 * the catalog registry supplies the model's box extents, colour and future mesh path;
 * `y` may be negative so props can be deliberately sunk into the floor, and `size`
 * overrides the catalog entry when a one-off box is wanted.
 */
class Prop {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('prop');
    // An explicitly empty model is preserved so validation can flag it instead of
    // silently repairing broken level data.
    this.model = data.model === undefined || data.model === null ? 'core:crate' : String(data.model);
    this.x = Number(data.x ?? 0);
    this.y = Number(data.y ?? 0);
    this.z = Number(data.z ?? 0);
    this.rotation_degrees = Number(data.rotation_degrees ?? 0);
    this.scale = Number(data.scale ?? 1);
    this.size = Array.isArray(data.size) && data.size.length === 3 ? data.size.map(Number) : null;
    this.solid = data.solid === true;
  }

  clone() {
    return new Prop({
      id: this.id,
      model: this.model,
      x: this.x,
      y: this.y,
      z: this.z,
      rotation_degrees: this.rotation_degrees,
      scale: this.scale,
      size: this.size ? this.size.slice() : null,
      solid: this.solid
    });
  }

  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('prop');
    return copy;
  }

  toJSON() {
    const obj = {
      model: this.model,
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      rotation_degrees: Number(this.rotation_degrees.toFixed(1))
    };
    if (this.y !== 0) obj.y = Number(this.y.toFixed(3));
    if (this.scale !== 1) obj.scale = Number(this.scale.toFixed(3));
    if (this.size) obj.size = this.size.map(v => Number(v.toFixed(3)));
    if (this.solid) obj.solid = true;
    return obj;
  }
}

class CeilingLight {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('light');
    this.fixture = data.fixture || 'core:fluorescent_panel_01';
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.rotation_degrees = Number(data.rotation_degrees ?? 0);
    // Fixture intensity. `brightness` is the canonical key the game and this
    // editor write; `intensity` is accepted as an alias for levels authored
    // from the design notes. Omitted means the standard 1.0 fixture.
    const intensity = data.brightness !== undefined && data.brightness !== null
      ? data.brightness
      : data.intensity;
    this.brightness = intensity !== undefined && intensity !== null ? Number(intensity) : 1.0;
  }

  clone() {
    return new CeilingLight({
      id: this.id,
      fixture: this.fixture,
      x: this.x,
      z: this.z,
      rotation_degrees: this.rotation_degrees,
      brightness: this.brightness
    });
  }

  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('light');
    return copy;
  }

  toJSON() {
    const obj = {
      fixture: this.fixture,
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      rotation_degrees: Number(this.rotation_degrees.toFixed(1))
    };
    if (this.brightness !== null && this.brightness !== undefined && this.brightness !== 1.0) {
      obj.brightness = Number(this.brightness.toFixed(2));
    }
    return obj;
  }
}

class Spawn {
  constructor(data = {}) {
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.yaw_degrees = Number(data.yaw_degrees ?? 0);
  }

  clone() {
    return new Spawn({
      x: this.x,
      z: this.z,
      yaw_degrees: this.yaw_degrees
    });
  }

  toJSON() {
    return {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      yaw_degrees: Number(this.yaw_degrees.toFixed(1))
    };
  }
}

class Room {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('room');
    this.x = Number(data.x ?? -10);
    this.z = Number(data.z ?? -10);
    this.width = Number(data.width ?? 20);
    this.depth = Number(data.depth ?? 20);
    this.height = Number(data.height ?? 3.5);
    // Optional per-room surface material overrides if specified
    this.material = data.material || null;
    this.ceiling_material = data.ceiling_material || null;
  }

  clone() {
    return new Room({
      id: this.id,
      x: this.x,
      z: this.z,
      width: this.width,
      depth: this.depth,
      height: this.height,
      material: this.material,
      ceiling_material: this.ceiling_material
    });
  }

  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('room');
    return copy;
  }

  toJSON() {
    const obj = {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3)),
      height: Number(this.height.toFixed(3))
    };
    if (this.material) obj.material = this.material;
    if (this.ceiling_material) obj.ceiling_material = this.ceiling_material;
    return obj;
  }
}

class FloorPatch {
  constructor(data = {}) {
    this.id = data.id || generateUniqueId('patch');
    this.x = Number(data.x ?? 0);
    this.z = Number(data.z ?? 0);
    this.width = Number(data.width ?? 2);
    this.depth = Number(data.depth ?? 2);
    this.material = data.material || 'core:carpet_damp_01';
  }

  clone() {
    return new FloorPatch({
      id: this.id,
      x: this.x,
      z: this.z,
      width: this.width,
      depth: this.depth,
      material: this.material
    });
  }

  duplicate() {
    const copy = this.clone();
    copy.id = generateUniqueId('patch');
    return copy;
  }

  toJSON() {
    return {
      x: Number(this.x.toFixed(3)),
      z: Number(this.z.toFixed(3)),
      width: Number(this.width.toFixed(3)),
      depth: Number(this.depth.toFixed(3)),
      material: this.material
    };
  }
}

class LevelDefaults {
  constructor(data = {}) {
    this.wall = data.wall || 'core:wallpaper_yellow_01';
    this.floor = data.floor || 'core:carpet_beige_01';
    this.ceiling = data.ceiling || 'core:ceiling_panel_01';
  }

  clone() {
    return new LevelDefaults({ ...this });
  }

  toJSON() {
    return {
      wall: this.wall,
      floor: this.floor,
      ceiling: this.ceiling
    };
  }
}

class Level {
  constructor(data = {}) {
    this.format_version = data.format_version || 1;
    this.id = data.id || 'new_level';
    this.name = data.name || 'New Level';
    this.author = data.author || '';
    this.spawn = new Spawn(data.spawn || { x: 0, z: 0, yaw_degrees: 0 });
    this.defaults = new LevelDefaults(data.defaults || {});
    
    // Support rooms array or single room
    this.rooms = [];
    if (data.rooms && Array.isArray(data.rooms) && data.rooms.length > 0) {
      this.rooms = data.rooms.map(r => new Room(r));
    } else if (data.room) {
      this.rooms = [new Room(data.room)];
    } else {
      this.rooms = [new Room({ x: -10, z: -10, width: 20, depth: 20, height: 3.5 })];
    }

    this.walls = (data.walls || []).map(w => new Wall(w));
    this.ceiling_lights = (data.ceiling_lights || []).map(l => new CeilingLight(l));
    this.floor_patches = (data.floor_patches || []).map(p => new FloorPatch(p));
    this.props = (data.props || []).map(p => new Prop(p));

    // Custom textures map: material_id -> { filename, dataUrl, width, height, bytes }
    this.custom_textures = {};
    if (data.custom_textures) {
      for (const [k, v] of Object.entries(data.custom_textures)) {
        this.custom_textures[k] = { ...v };
      }
    }
  }

  getCeilingHeight(x = 0, z = 0) {
    for (const r of this.rooms) {
      const minX = Math.min(r.x, r.x + r.width);
      const maxX = Math.max(r.x, r.x + r.width);
      const minZ = Math.min(r.z, r.z + r.depth);
      const maxZ = Math.max(r.z, r.z + r.depth);
      if (x >= minX - 0.01 && x <= maxX + 0.01 && z >= minZ - 0.01 && z <= maxZ + 0.01) {
        return r.height;
      }
    }
    return this.rooms[0]?.height || 3.5;
  }

  clone() {
    return new Level({
      format_version: this.format_version,
      id: this.id,
      name: this.name,
      author: this.author,
      spawn: this.spawn.clone(),
      defaults: this.defaults.clone(),
      rooms: this.rooms.map(r => r.clone()),
      walls: this.walls.map(w => w.clone()),
      ceiling_lights: this.ceiling_lights.map(l => l.clone()),
      floor_patches: this.floor_patches.map(p => p.clone()),
      props: this.props.map(p => p.clone()),
      custom_textures: JSON.parse(JSON.stringify(this.custom_textures))
    });
  }

  toJSON() {
    const result = {
      format_version: this.format_version,
      id: this.id,
      name: this.name,
      author: this.author,
      spawn: this.spawn.toJSON(),
      defaults: this.defaults.toJSON()
    };

    if (this.rooms.length === 1) {
      result.room = this.rooms[0].toJSON();
    } else {
      result.rooms = this.rooms.map(r => r.toJSON());
    }

    result.walls = this.walls.map(w => w.toJSON());

    if (this.floor_patches.length > 0) {
      result.floor_patches = this.floor_patches.map(p => p.toJSON());
    }

    result.ceiling_lights = this.ceiling_lights.map(l => l.toJSON());

    if (this.props.length > 0) {
      result.props = this.props.map(p => p.toJSON());
    }

    return result;
  }
}

// Level Validation logic mirroring liminal-rust loader.rs validate_level
function openingLabel(kind) {
  switch (kind) {
    case 'window': return 'Window';
    case 'passage': return 'Passage';
    case 'vent': return 'Vent';
    default: return 'Door';
  }
}

function validateLevel(level) {
  const errors = [];
  const warnings = [];

  // 1. Format version
  if (level.format_version !== 1) {
    errors.push(`Unsupported format_version: ${level.format_version} (expected 1)`);
  }

  // 2. Identity
  if (!level.id || !level.id.trim()) {
    errors.push("Level 'id' cannot be empty");
  } else if (!/^[a-zA-Z0-9_-]+$/.test(level.id.trim())) {
    warnings.push("Level 'id' should use only letters, numbers, underscores, and hyphens");
  }

  if (!level.name || !level.name.trim()) {
    errors.push("Level 'name' cannot be empty");
  }

  // 3. Player Spawn
  if (!level.spawn) {
    errors.push("Player spawn definition is missing");
  } else {
    if (!Number.isFinite(level.spawn.x) || !Number.isFinite(level.spawn.z)) {
      errors.push("Player spawn coordinates must be finite numbers");
    }
    if (!Number.isFinite(level.spawn.yaw_degrees)) {
      errors.push("Player spawn yaw_degrees must be a finite number");
    }

    // Check if spawn is inside room bounds
    if (level.rooms.length > 0) {
      let insideAnyRoom = false;
      for (const r of level.rooms) {
        const x0 = Math.min(r.x, r.x + r.width);
        const x1 = Math.max(r.x, r.x + r.width);
        const z0 = Math.min(r.z, r.z + r.depth);
        const z1 = Math.max(r.z, r.z + r.depth);
        if (level.spawn.x >= x0 && level.spawn.x <= x1 && level.spawn.z >= z0 && level.spawn.z <= z1) {
          insideAnyRoom = true;
          break;
        }
      }
      if (!insideAnyRoom) {
        warnings.push("Player spawn point is outside all room boundaries");
      }
    }
  }

  // 4. Geometry limits (liminal-rust limits)
  if (level.rooms.length > 500) {
    errors.push(`Level contains too many rooms: ${level.rooms.length} (limit: 500)`);
  }
  if (level.walls.length > 5000) {
    errors.push(`Level contains too many walls: ${level.walls.length} (limit: 5000)`);
  }
  if (level.ceiling_lights.length > 5000) {
    errors.push(`Level contains too many ceiling lights: ${level.ceiling_lights.length} (limit: 5000)`);
  }

  level.rooms.forEach((r, i) => {
    if (!Number.isFinite(r.width) || !Number.isFinite(r.depth) || !Number.isFinite(r.height)) {
      errors.push(`Room ${i} dimensions must be finite numbers`);
    } else {
      if (r.width <= 0 || r.depth <= 0 || r.height <= 0) {
        errors.push(`Room ${i} width, depth, and height must be positive numbers`);
      }
      if (r.width > 2000 || r.depth > 2000 || r.height > 50) {
        errors.push(`Room ${i} dimensions exceed maximum limits (max 2000x2000x50m)`);
      }
    }
  });

  level.walls.forEach((w, i) => {
    if (!Number.isFinite(w.x) || !Number.isFinite(w.y) || !Number.isFinite(w.z) || !Number.isFinite(w.width) || !Number.isFinite(w.depth)) {
      errors.push(`Wall ${i} position/dimensions must be finite numbers`);
    } else {
      if (w.width <= 0 || w.depth <= 0) {
        errors.push(`Wall ${i} width and depth must be positive numbers (> 0)`);
      }
      if (w.height !== null && w.height !== undefined) {
        if (!Number.isFinite(w.height)) {
          errors.push(`Wall ${i} height must be a finite number`);
        } else if (w.height <= 0) {
          errors.push(`Wall ${i} height must be positive (> 0)`);
        }
      }
    }

    // Wall openings (doors, windows, passages, vents). Messages mirror the game
    // loader so a level the editor accepts is accepted by liminal-rust.
    const wallLength = Math.max(Math.abs(w.width), Math.abs(w.depth));
    (w.openings || []).forEach((o, j) => {
      const label = openingLabel(o.kind);
      if (![o.offset, o.width, o.height, o.sill].every(Number.isFinite)) {
        errors.push(`Wall ${i} ${label} opening contains non-finite numbers`);
        return;
      }
      if (o.width <= 0 || o.height <= 0) {
        errors.push(`Wall ${i} ${label} opening must have a positive width and height`);
      }
      if (o.sill < 0) {
        errors.push(`Wall ${i} ${label} opening cannot have a negative sill height`);
      }
      if (o.offset < 0) {
        errors.push(`Wall ${i} ${label} opening starts before the wall`);
      }
      if (o.offset + o.width > wallLength + 0.001) {
        errors.push(`${label} opening extends beyond this wall (wall ${i}: opening ends at ${(o.offset + o.width).toFixed(2)} m, wall is ${wallLength.toFixed(2)} m long)`);
      }
    });
  });

  // Props: registry-driven placement. Intentional clipping (props inside walls or
  // sunk below the floor) is allowed; only malformed data is rejected.
  const props = level.props || [];
  if (props.length > 5000) {
    errors.push(`Level contains too many props: ${props.length} (limit: 5000)`);
  }
  props.forEach((p, i) => {
    if (!p.model || !String(p.model).trim()) {
      errors.push(`Prop ${i} must reference a non-empty model id`);
    }
    if (![p.x, p.y, p.z, p.rotation_degrees, p.scale].every(Number.isFinite)) {
      errors.push(`Prop ${i} position, rotation, and scale must be finite numbers`);
    } else if (p.scale <= 0) {
      errors.push(`Prop ${i} scale must be positive`);
    }
    if (p.size && !p.size.every(v => Number.isFinite(v) && v > 0)) {
      errors.push(`Prop ${i} size must contain positive finite numbers`);
    }
  });

  // 5. Ceiling lights: fixtures, positions and optional intensity. `brightness`
  // is the canonical key (also accepted as `intensity` when importing); omitted
  // means the standard 1.0 fixture. Messages mirror the game loader.
  level.ceiling_lights.forEach((l, i) => {
    if (!Number.isFinite(l.x) || !Number.isFinite(l.z) || !Number.isFinite(l.rotation_degrees)) {
      errors.push(`Ceiling light ${i} position and rotation must be finite numbers`);
    }
    if (!l.fixture || !String(l.fixture).trim()) {
      errors.push(`Ceiling light ${i} must reference a non-empty fixture id`);
    }
    if (l.brightness !== null && l.brightness !== undefined) {
      if (!Number.isFinite(l.brightness)) {
        errors.push(`Ceiling light ${i} intensity must be a finite number`);
      } else if (l.brightness < 0) {
        errors.push(`Ceiling light ${i} intensity cannot be negative`);
      } else if (l.brightness > LIGHT_INTENSITY_MAX) {
        warnings.push(`Ceiling light ${i} intensity ${l.brightness} is above ${LIGHT_INTENSITY_MAX} and will be clamped by the game`);
      }
    }
  });

  // 6. Materials validation
  const knownMaterials = new Set(Object.keys(CORE_MATERIALS));
  if (level.custom_textures) {
    for (const id of Object.keys(level.custom_textures)) {
      knownMaterials.add(id);
    }
  }

  const checkMaterial = (matId, context) => {
    if (!matId) return;
    if (matId.startsWith('pack:') && !knownMaterials.has(matId)) {
      warnings.push(`Missing texture reference: "${matId}" used in ${context}`);
    } else if (!matId.startsWith('core:') && !matId.startsWith('pack:')) {
      warnings.push(`Unrecognized material namespace: "${matId}" in ${context} (expected "core:" or "pack:")`);
    }
  };

  checkMaterial(level.defaults.wall, 'defaults.wall');
  checkMaterial(level.defaults.floor, 'defaults.floor');
  checkMaterial(level.defaults.ceiling, 'defaults.ceiling');

  level.walls.forEach((w, i) => {
    if (w.material) checkMaterial(w.material, `Wall ${i}`);
    if (w.faces) {
      for (const [face, mat] of Object.entries(w.faces)) {
        checkMaterial(mat, `Wall ${i} face "${face}"`);
      }
    }
  });

  level.ceiling_lights.forEach((l, i) => {
    checkMaterial(l.fixture, `Ceiling light ${i}`);
  });

  level.floor_patches.forEach((p, i) => {
    checkMaterial(p.material, `Floor patch ${i}`);
  });

  // 7. Texture dimensions check
  if (level.custom_textures) {
    for (const [id, tex] of Object.entries(level.custom_textures)) {
      if (tex.width > 1024 || tex.height > 1024) {
        errors.push(`Custom texture "${id}" dimensions ${tex.width}x${tex.height} exceed engine limit of 1024x1024`);
      }
    }
  }

  return {
    valid: errors.length === 0,
    errors,
    warnings
  };
}

// Global initialization
if (typeof window !== 'undefined') {
  initCoreThumbnails();
}

// Class and const declarations are lexically scoped in the browser, so other
// scripts can reference them by name but they are not window properties. ops.js
// resolves its dependencies through the global object, so publish them explicitly.
if (typeof window !== 'undefined') {
  Object.assign(window, {
    CORE_MATERIALS,
    CORE_THUMBNAILS,
    WallOpening,
    Wall,
    CeilingLight,
    Spawn,
    Room,
    FloorPatch,
    Prop,
    LevelDefaults,
    Level,
    openingLabel,
    validateLevel,
    generateUniqueId,
    LIGHT_INTENSITY_MAX
  });
}

if (typeof module !== 'undefined' && module.exports) {
  module.exports = {
    CORE_MATERIALS,
    WallOpening,
    Wall,
    CeilingLight,
    Spawn,
    Room,
    FloorPatch,
    Prop,
    LevelDefaults,
    Level,
    openingLabel,
    validateLevel,
    LIGHT_INTENSITY_MAX
  };
}
