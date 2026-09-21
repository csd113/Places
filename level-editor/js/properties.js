// properties.js - Contextual inspector.
//
// Shows only what the current selection needs. Simple fields first; the rest lives
// inside an Advanced <details> section (which the app hides entirely in simple mode).
// Nothing here mutates the level directly except through small, explicit helpers.

class PropertiesPanel {
  constructor(container, app) {
    this.container = container;
    this.app = app;
    this.renderedKey = null;
    this.inputs = new Map();
    this.bindEvents();
    this.render();
  }

  // ---------------------------------------------------------------- helpers

  esc(value) {
    return String(value === undefined || value === null ? '' : value)
      .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  }

  num(value, digits = 2) {
    const n = Number(value);
    return Number.isFinite(n) ? n.toFixed(digits) : '0';
  }

  input(obj, field, value, opts = {}) {
    const step = opts.step === undefined ? 0.1 : opts.step;
    return `<input type="number" step="${step}" ${opts.min !== undefined ? `min="${opts.min}"` : ''}
      ${opts.max !== undefined ? `max="${opts.max}"` : ''}
      data-obj="${obj}" data-field="${field}" data-label="${this.esc(opts.label || field)}"
      ${opts.id ? `data-id="${opts.id}"` : ''}
      value="${this.num(value, opts.digits === undefined ? 2 : opts.digits)}">`;
  }

  text(obj, field, value, label) {
    return `<input type="text" data-obj="${obj}" data-field="${field}" data-label="${this.esc(label || field)}" value="${this.esc(value)}">`;
  }

  field(label, inner, note) {
    return `<div class="field"><label>${this.esc(label)}</label>${inner}${note ? `<div class="field-note">${note}</div>` : ''}</div>`;
  }

  fieldRow(label, inner) {
    return `<div class="field"><label>${this.esc(label)}</label><div class="field-row">${inner}</div></div>`;
  }

  section(title) {
    return `<h5>${this.esc(title)}</h5>`;
  }

  advanced(content) {
    if (!content) return '';
    return `<details class="advanced"><summary>Advanced</summary>${content}</details>`;
  }

  actionRow(buttons) {
    return `<div class="action-row">${buttons.join('')}</div>`;
  }

  button(label, action, args = {}, cls = 'btn') {
    const attrs = Object.entries(args).map(([k, v]) => `data-${k}="${this.esc(v)}"`).join(' ');
    return `<button class="${cls}" data-action="${action}" ${attrs}>${this.esc(label)}</button>`;
  }

  pills(entries, activeValue, action) {
    return `<div class="pill-row">${entries.map(([label, value]) =>
      `<button class="pill ${String(value) === String(activeValue) ? 'active' : ''}" data-action="${action}" data-value="${value}">${this.esc(label)}</button>`
    ).join('')}</div>`;
  }

  materialOptions(category, selected, includeDefault) {
    const level = this.app.level;
    const rows = [];
    if (includeDefault) rows.push(`<option value=""${!selected ? ' selected' : ''}>Level default</option>`);
    for (const material of Object.values(CORE_MATERIALS)) {
      if (category && material.category !== category) continue;
      rows.push(`<option value="${material.id}"${selected === material.id ? ' selected' : ''}>${this.esc(material.name)}</option>`);
    }
    const packs = Object.keys(level.custom_textures || {});
    if (packs.length > 0) {
      rows.push('<optgroup label="Imported textures">');
      for (const id of packs) {
        rows.push(`<option value="${id}"${selected === id ? ' selected' : ''}>${this.esc(id)}</option>`);
      }
      rows.push('</optgroup>');
    }
    const known = Object.values(CORE_MATERIALS).some(m => m.id === selected) || packs.includes(selected);
    if (selected && !known) {
      rows.push(`<option value="${this.esc(selected)}" selected>${this.esc(selected)}</option>`);
    }
    return rows.join('');
  }

  materialField(label, obj, field, category, selected, includeDefault) {
    const swatch = this.materialColor(selected);
    return `<div class="field">
      <label>${this.esc(label)}</label>
      <div class="field-row">
        <select data-obj="${obj}" data-field="${field}" data-label="${this.esc(label)}">${this.materialOptions(category, selected, includeDefault)}</select>
        <span class="swatch" style="background:${swatch}"></span>
      </div>
    </div>`;
  }

  materialColor(id) {
    const material = CORE_MATERIALS[id];
    if (material) return material.color;
    if (id && this.app.level.custom_textures && this.app.level.custom_textures[id]) return '#7f8c8d';
    return '#4a4f55';
  }

  // ----------------------------------------------------------------- render

  render() {
    const selection = this.app.editor.selectedIds;
    const key = selection.size === 0 ? 'level' : selection.size === 1 ? [...selection][0] : `multi:${[...selection].sort().join(',')}`;
    this.renderedKey = key;
    this.inputs.clear();

    let html;
    if (selection.size === 0) html = this.renderLevel();
    else if (selection.size === 1) html = this.renderObject([...selection][0]) || this.renderLevel();
    else html = this.renderMulti([...selection]);

    this.container.innerHTML = html;
  }

  /**
   * Updates displayed values without rebuilding the DOM, so dragging in the views
   * does not destroy focus or flash the panel.
   */
  syncValues() {
    const selection = this.app.editor.selectedIds;
    const key = selection.size === 0 ? 'level' : selection.size === 1 ? [...selection][0] : `multi:${[...selection].sort().join(',')}`;
    if (key !== this.renderedKey) {
      this.render();
      return;
    }
    for (const el of this.container.querySelectorAll('[data-obj][data-field]')) {
      if (el === document.activeElement) continue;
      const value = this.readField(el.dataset.obj, el.dataset.field, el.dataset.id);
      if (value === null || value === undefined) continue;
      if (el.type === 'checkbox') el.checked = !!value;
      else if (Number.isFinite(Number(value)) && el.type === 'number') el.value = Number(value).toFixed(el.dataset.digits === undefined ? 2 : Number(el.dataset.digits));
      else if (el.tagName === 'SELECT' || el.type === 'text' || el.type === 'number') el.value = value;
      const swatch = el.parentElement && el.parentElement.querySelector('.swatch');
      if (swatch && el.tagName === 'SELECT') swatch.style.background = this.materialColor(el.value);
    }
  }

  // ------------------------------------------------------------ level panel

  renderLevel() {
    const level = this.app.level;
    const stats = LiminalGeometry.levelStats(level);
    const validation = validateLevel(level);
    const problemCount = validation.errors.length + validation.warnings.length;

    return `
      <h4>Level</h4>
      <div class="fields">
        ${this.field('Name', this.text('level', 'name', level.name, 'Name'))}
      </div>

      <h5>Level surfaces</h5>
      <div class="fields">
        ${this.materialField('Floor material', 'defaults', 'floor', 'floor', level.defaults.floor, false)}
        ${this.materialField('Wall material', 'defaults', 'wall', 'wall', level.defaults.wall, false)}
        ${this.materialField('Ceiling material', 'defaults', 'ceiling', 'ceiling', level.defaults.ceiling, false)}
      </div>

      <h5>In this level</h5>
      <div class="stat-grid">
        <span>Rooms</span><strong>${stats.rooms}</strong>
        <span>Walls</span><strong>${stats.walls}</strong>
        <span>Doors / windows</span><strong>${stats.openings}</strong>
        <span>Lights</span><strong>${stats.lights}</strong>
        <span>Props</span><strong>${stats.props}</strong>
      </div>

      <div class="action-row">
        ${this.button('+ Add room', 'add-room')}
        ${this.button('+ Add light', 'add-light')}
      </div>
      <div class="action-row">
        ${this.button('Add walls around all rooms', 'walls-for-rooms', {}, 'btn btn-block')}
      </div>

      <p class="hint">Nothing selected. Click an object to edit it, or pick a tool on the left to build.</p>

      ${this.advanced(`
        <div class="fields">
          ${this.field('Level ID', this.text('level', 'id', level.id, 'Level ID'))}
          ${this.field('Author', this.text('level', 'author', level.author, 'Author'))}
          ${this.field('Format version', `<input type="text" value="${level.format_version}" disabled>`)}
        </div>

        ${this.section(`Room sections (${level.rooms.length})`)}
        <div class="list">
          ${level.rooms.map((room, index) => `
            <div class="list-item">
              <span class="grow">Room ${index + 1} · ${this.num(room.width, 1)}×${this.num(room.depth, 1)} m</span>
              ${this.button('Select', 'select-id', { id: room.id }, 'btn btn-sm')}
            </div>`).join('')}
        </div>

        ${level.floor_patches.length > 0 ? `
          ${this.section(`Floor patches (${level.floor_patches.length})`)}
          <div class="list">
            ${level.floor_patches.map((patch, index) => `
              <div class="list-item">
                <span class="grow">Patch ${index + 1} · ${this.num(patch.width, 1)}×${this.num(patch.depth, 1)} m</span>
                ${this.button('Select', 'select-id', { id: patch.id }, 'btn btn-sm')}
              </div>`).join('')}
          </div>` : ''}

        ${this.section(`Imported textures (${Object.keys(level.custom_textures || {}).length})`)}
        <input type="file" id="texture-input" accept="image/png,image/jpeg,image/webp" hidden>
        ${this.button('+ Import texture…', 'import-texture', {}, 'btn btn-block')}
        <div class="list" style="margin-top:6px">
          ${Object.keys(level.custom_textures || {}).map(id => `
            <div class="list-item">
              <span class="grow">${this.esc(id)}</span>
              ${this.button('Remove', 'remove-texture', { id }, 'btn btn-sm btn-danger')}
            </div>`).join('') || '<p class="hint">No imported textures.</p>'}
        </div>
      `)}

      <div class="action-row">
        ${this.button(problemCount === 0 ? '✓ Level looks valid' : `Check level (${problemCount} issue${problemCount === 1 ? '' : 's'})`, 'validate',
          {}, problemCount === 0 ? 'btn btn-block' : 'btn btn-block btn-primary')}
      </div>
    `;
  }

  // ----------------------------------------------------------- object panels

  renderObject(id) {
    const level = this.app.level;
    if (id === 'spawn') return this.renderSpawn();
    const wall = level.walls.find(w => w.id === id);
    if (wall) return this.renderWall(wall);
    const room = level.rooms.find(r => r.id === id);
    if (room) return this.renderRoom(room);
    const light = level.ceiling_lights.find(l => l.id === id);
    if (light) return this.renderLight(light);
    const prop = level.props.find(p => p.id === id);
    if (prop) return this.renderProp(prop);
    const patch = level.floor_patches.find(p => p.id === id);
    if (patch) return this.renderPatch(patch);
    const openingRef = LiminalOps.findOpening(level, id);
    if (openingRef) return this.renderOpening(openingRef.wall, openingRef.opening);
    return null;
  }

  header(label, cls, id) {
    return `<div class="type"><span class="type-badge ${cls}">${this.esc(label)}</span>
      <span class="type-id">${this.esc((id || '').slice(0, 12))}</span></div>`;
  }

  actions(id, extra = []) {
    return this.actionRow([
      ...extra,
      this.button('Duplicate', 'duplicate', {}, 'btn'),
      this.button('Delete', 'delete', {}, 'btn btn-danger')
    ]);
  }

  renderRoom(room) {
    const level = this.app.level;
    return `
      <h4>Room</h4>
      ${this.header(`Room · ${this.num(room.width, 1)}×${this.num(room.depth, 1)} m`, 'room', room.id)}
      <div class="fields two">
        ${this.field('Width (m)', this.input('room', 'width', room.width, { min: 0.5, step: 0.5 }))}
        ${this.field('Length (m)', this.input('room', 'depth', room.depth, { min: 0.5, step: 0.5 }))}
      </div>
      <div class="fields">
        ${this.field('Ceiling height (m)', this.input('room', 'height', room.height, { min: 1, max: 50, step: 0.1 }),
          'Walls at full height follow this value.')}
        ${this.materialField('Floor material', 'room', 'material', 'floor', room.material || '', true)}
        ${this.materialField('Ceiling material', 'room', 'ceiling_material', 'ceiling', room.ceiling_material || '', true)}
      </div>
      ${this.pills([['2.8 m', 2.8], ['3.5 m', 3.5], ['4.5 m', 4.5], ['6 m', 6]], room.height, 'set-height')}
      ${this.actions(room.id, [this.button('Add walls around', 'walls-for-room')])}
      ${this.advanced(`
        <div class="fields two">
          ${this.field('Position X (m)', this.input('room', 'x', room.x, { step: 0.1 }))}
          ${this.field('Position Z (m)', this.input('room', 'z', room.z, { step: 0.1 }))}
        </div>
        <div class="fields two">
          ${this.field('Floor material ID', this.text('room', 'material', room.material || '', 'Floor material ID'))}
          ${this.field('Ceiling material ID', this.text('room', 'ceiling_material', room.ceiling_material || '', 'Ceiling material ID'))}
        </div>
        <p class="field-note">Per-room materials are stored in level.json for tooling; the game currently renders floors and ceilings from the level defaults.</p>
        <div class="fields">${this.field('Object ID', `<input type="text" value="${this.esc(room.id)}" disabled>`)}</div>
      `)}
    `;
  }

  renderWall(wall) {
    const axis = LiminalGeometry.wallAxis(wall);
    const length = LiminalGeometry.wallLength(wall);
    const thickness = LiminalGeometry.wallThickness(wall);
    const level = this.app.level;
    const fullHeight = wall.height === null || wall.height === undefined;
    const resolved = LiminalGeometry.wallResolvedHeight(wall, level.getCeilingHeight());

    return `
      <h4>Wall</h4>
      ${this.header(`Wall · ${this.num(length, 2)} m`, 'wall', wall.id)}
      <div class="fields two">
        ${this.field('Length (m)', `<input type="number" step="0.1" min="0.05" data-obj="wall" data-field="length" value="${this.num(length, 2)}">`)}
        ${this.field('Thickness (m)', `<input type="number" step="0.05" min="0.05" data-obj="wall" data-field="thickness" value="${this.num(thickness, 2)}">`)}
      </div>
      <div class="fields">
        <label class="switch"><input type="checkbox" data-obj="wall" data-field="fullHeight" ${fullHeight ? 'checked' : ''}><span>Full ceiling height (${this.num(level.getCeilingHeight(), 1)} m)</span></label>
        ${fullHeight ? '' : this.field('Height (m)', this.input('wall', 'height', resolved, { min: 0.1, step: 0.1 }))}
        ${this.materialField('Material', 'wall', 'material', 'wall', wall.material || '', true)}
      </div>

      ${this.section(`Openings (${wall.openings.length})`)}
      <div class="list">
        ${wall.openings.map(opening => `
          <div class="list-item">
            <span class="grow">${this.esc(LiminalGeometry.OPENING_KINDS[opening.kind] ? LiminalGeometry.OPENING_KINDS[opening.kind].label : opening.kind)} · ${this.num(opening.width, 2)} m</span>
            ${this.button('Edit', 'select-id', { id: opening.id }, 'btn btn-sm')}
            ${this.button('✕', 'remove-opening', { id: opening.id }, 'btn btn-sm')}
          </div>`).join('') || '<p class="hint">No openings yet. With the Door or Window tool, click this wall.</p>'}
      </div>
      ${this.actions(wall.id, [
        this.button('+ Door', 'add-opening', { kind: 'door' }, 'btn btn-sm'),
        this.button('+ Window', 'add-opening', { kind: 'window' }, 'btn btn-sm')
      ])}

      ${this.advanced(`
        <div class="fields two">
          ${this.field('Position X (m)', this.input('wall', 'x', wall.x, { step: 0.05 }))}
          ${this.field('Position Z (m)', this.input('wall', 'z', wall.z, { step: 0.05 }))}
        </div>
        <div class="fields two">
          ${this.field('Exact width (m)', this.input('wall', 'width', wall.width, { min: 0.05, step: 0.05 }))}
          ${this.field('Exact depth (m)', this.input('wall', 'depth', wall.depth, { min: 0.05, step: 0.05 }))}
        </div>
        <div class="fields two">
          ${this.field('Base elevation Y (m)', this.input('wall', 'y', wall.y, { step: 0.05 }), 'Raise for headers, drop for sunken ledges.')}
          ${this.field('Exact height (m)', this.input('wall', 'height', resolved, { min: 0.1, step: 0.05 }))}
        </div>
        ${this.section('Per-face materials')}
        <div class="fields two">
          ${['north', 'south', 'east', 'west'].map(face => this.field(face[0].toUpperCase() + face.slice(1), `
            <input type="text" data-obj="wall" data-field="face-${face}" data-label="${face} face" value="${this.esc(wall.faces[face] || '')}" placeholder="inherit">`)).join('')}
        </div>
        <div class="fields">${this.field('Object ID', `<input type="text" value="${this.esc(wall.id)}" disabled>`)}</div>
      `)}
    `;
  }

  renderOpening(wall, opening) {
    const length = LiminalGeometry.wallLength(wall);
    const isWindow = opening.kind === 'window' || opening.kind === 'vent';
    const preset = LiminalGeometry.OPENING_KINDS[opening.kind] || LiminalGeometry.OPENING_KINDS.door;
    const maxWidth = Math.max(0.2, length - opening.offset);
    return `
      <h4>${this.esc(preset.label)}</h4>
      ${this.header(`${preset.label} in wall · ${this.num(opening.width, 2)} m`, opening.kind, opening.id)}
      <div class="fields">
        ${this.field('Type', `<select data-obj="opening" data-field="kind">
          ${Object.entries(LiminalGeometry.OPENING_KINDS).map(([kind, def]) =>
            `<option value="${kind}"${opening.kind === kind ? ' selected' : ''}>${def.label}</option>`).join('')}
        </select>`)}
      </div>
      <div class="fields two">
        ${this.field('Width (m)', this.input('opening', 'width', opening.width, { min: 0.2, max: maxWidth, step: 0.05 }))}
        ${this.field('Height (m)', this.input('opening', 'height', opening.height, { min: 0.2, step: 0.05 }))}
      </div>
      ${this.fieldRow('Position along wall (m)', `
        <input type="range" min="0" max="${Math.max(0, length - opening.width).toFixed(2)}" step="0.05"
          data-obj="opening" data-field="offset" data-label="Position" value="${this.num(opening.offset, 2)}">
        <input type="number" min="0" max="${Math.max(0, length - opening.width).toFixed(2)}" step="0.05"
          data-obj="opening" data-field="offset" value="${this.num(opening.offset, 2)}">`)}
      ${isWindow ? this.field('Sill height (m)', this.input('opening', 'sill', opening.sill, { min: 0, step: 0.05 })) : ''}
      ${this.actions(opening.id, [this.button('Select wall', 'select-wall')])}
      ${this.advanced(`
        <div class="fields two">
          ${this.field('Exact offset (m)', this.input('opening', 'offset', opening.offset, { min: 0, step: 0.01 }))}
          ${this.field('Exact width (m)', this.input('opening', 'width', opening.width, { min: 0.2, step: 0.01 }))}
        </div>
        <div class="fields two">
          ${this.field('Exact height (m)', this.input('opening', 'height', opening.height, { min: 0.2, step: 0.01 }))}
          ${this.field('Sill height (m)', this.input('opening', 'sill', opening.sill, { min: 0, step: 0.01 }))}
        </div>
        <p class="field-note">Wall length ${this.num(length, 2)} m · kind "${this.esc(opening.kind)}".
          The opening always cuts through the full wall thickness.</p>
        <div class="fields">${this.field('Object ID', `<input type="text" value="${this.esc(opening.id)}" disabled>`)}</div>
      `)}
    `;
  }

  renderLight(light) {
    const level = this.app.level;
    return `
      <h4>Ceiling light</h4>
      ${this.header('Ceiling light', 'light', light.id)}
      <div class="fields two">
        ${this.field('Position X (m)', this.input('light', 'x', light.x, { step: 0.1 }))}
        ${this.field('Position Z (m)', this.input('light', 'z', light.z, { step: 0.1 }))}
      </div>
      <div class="fields">
        ${this.field('Brightness', this.input('light', 'brightness', light.brightness, { min: 0.1, max: 5, step: 0.1 }))}
        <label class="switch"><input type="checkbox" data-obj="light" data-field="turned" ${Math.round(light.rotation_degrees / 90) % 2 !== 0 ? 'checked' : ''}><span>Turned 90°</span></label>
      </div>
      <p class="hint">Fits the ceiling at ${this.num(level.getCeilingHeight(light.x, light.z), 1)} m.</p>
      ${this.actions(light.id)}
      ${this.advanced(`
        <div class="fields two">
          ${this.field('Rotation (degrees)', this.input('light', 'rotation_degrees', light.rotation_degrees, { step: 15 }))}
          ${this.field('Fixture ID', this.text('light', 'fixture', light.fixture, 'Fixture ID'))}
        </div>
        <div class="fields">${this.field('Object ID', `<input type="text" value="${this.esc(light.id)}" disabled>`)}</div>
      `)}
    `;
  }

  renderProp(prop) {
    const catalog = this.app.propCatalog;
    const entry = catalog.get(prop.model);
    const groups = catalog.categories();
    return `
      <h4>Prop</h4>
      ${this.header(`Prop · ${this.esc(entry.name)}`, 'prop', prop.id)}
      <div class="fields">
        ${this.field('Model', `<select data-obj="prop" data-field="model">
          ${groups.map(category => `<optgroup label="${this.esc(category)}">${catalog.search('', category).map(item =>
            `<option value="${item.id}"${prop.model === item.id ? ' selected' : ''}>${this.esc(item.name)}</option>`).join('')}</optgroup>`).join('')}
          ${entry.missing ? `<option value="${this.esc(prop.model)}" selected>${this.esc(prop.model)} (missing)</option>` : ''}
        </select>`)}
        ${entry.model ? `<p class="field-note">Mesh asset reserved: <code>${this.esc(entry.model)}</code></p>` : ''}
      </div>
      <div class="fields two">
        ${this.field('Position X (m)', this.input('prop', 'x', prop.x, { step: 0.1 }))}
        ${this.field('Position Z (m)', this.input('prop', 'z', prop.z, { step: 0.1 }))}
      </div>
      <div class="fields">
        ${this.field('Rotation (degrees)', this.input('prop', 'rotation_degrees', prop.rotation_degrees, { step: 15 }))}
      </div>
      ${this.pills([['0°', 0], ['90°', 90], ['180°', 180], ['270°', 270]], Math.round(prop.rotation_degrees) % 360, 'set-rotation')}
      ${this.actions(prop.id)}
      ${this.advanced(`
        <div class="fields two">
          ${this.field('Vertical offset Y (m)', this.input('prop', 'y', prop.y, { step: 0.05 }), 'Negative sinks the prop into the floor.')}
          ${this.field('Scale', this.input('prop', 'scale', prop.scale, { min: 0.05, step: 0.05 }))}
        </div>
        <div class="fields two">
          <label class="switch"><input type="checkbox" data-obj="prop" data-field="solid" ${prop.solid ? 'checked' : ''}><span>Blocks the player</span></label>
        </div>
        <div class="fields two">
          ${[0, 1, 2].map(i => this.field(['Size width (m)', 'Size height (m)', 'Size depth (m)'][i],
            `<input type="number" step="0.05" min="0.01" data-obj="prop" data-field="size${i}" value="${this.num((prop.size || entry.size)[i], 2)}">`)).join('')}
        </div>
        <p class="field-note">Catalog size: ${entry.size.map(v => this.num(v, 2)).join(' × ')} m${entry.missing ? ' (unknown model, placeholder box)' : ''}</p>
        <div class="fields">${this.field('Object ID', `<input type="text" value="${this.esc(prop.id)}" disabled>`)}</div>
      `)}
    `;
  }

  renderSpawn() {
    const spawn = this.app.level.spawn;
    return `
      <h4>Player spawn</h4>
      ${this.header('Player spawn', 'spawn', 'spawn')}
      <div class="fields two">
        ${this.field('Position X (m)', this.input('spawn', 'x', spawn.x, { step: 0.1 }))}
        ${this.field('Position Z (m)', this.input('spawn', 'z', spawn.z, { step: 0.1 }))}
      </div>
      <div class="fields">
        ${this.field('Facing (degrees)', this.input('spawn', 'yaw_degrees', spawn.yaw_degrees, { step: 15 }))}
      </div>
      ${this.pills([['North', 0], ['East', 90], ['South', 180], ['West', 270]], Math.round(spawn.yaw_degrees) % 360, 'set-yaw')}
      <p class="hint">The player always starts here. Only one spawn exists per level.</p>
    `;
  }

  renderPatch(patch) {
    return `
      <h4>Floor patch</h4>
      ${this.header('Floor patch', 'room', patch.id)}
      <div class="fields two">
        ${this.field('Width (m)', this.input('patch', 'width', patch.width, { min: 0.25, step: 0.25 }))}
        ${this.field('Length (m)', this.input('patch', 'depth', patch.depth, { min: 0.25, step: 0.25 }))}
      </div>
      ${this.materialField('Material', 'patch', 'material', 'floor', patch.material, false)}
      ${this.actions(patch.id)}
      ${this.advanced(`
        <div class="fields two">
          ${this.field('Position X (m)', this.input('patch', 'x', patch.x, { step: 0.1 }))}
          ${this.field('Position Z (m)', this.input('patch', 'z', patch.z, { step: 0.1 }))}
        </div>
      `)}
    `;
  }

  renderMulti(ids) {
    const level = this.app.level;
    const counts = { room: 0, wall: 0, light: 0, prop: 0, opening: 0, spawn: 0 };
    for (const id of ids) {
      if (id === 'spawn') counts.spawn++;
      else if (level.rooms.some(r => r.id === id)) counts.room++;
      else if (level.walls.some(w => w.id === id)) counts.wall++;
      else if (level.ceiling_lights.some(l => l.id === id)) counts.light++;
      else if (level.props.some(p => p.id === id)) counts.prop++;
      else if (LiminalOps.findOpening(level, id)) counts.opening++;
    }
    const summary = Object.entries(counts).filter(([, n]) => n > 0)
      .map(([kind, n]) => `${n} ${kind}${n === 1 ? '' : 's'}`).join(' · ');

    return `
      <h4>${ids.length} objects selected</h4>
      <p class="hint">${this.esc(summary)}</p>
      <div class="fields">
        ${this.fieldRow('Move by (m)', `
          <input type="number" id="multi-dx" step="0.5" value="0.5" title="X">
          <input type="number" id="multi-dz" step="0.5" value="0" title="Z">
          ${this.button('Move', 'multi-move', {}, 'btn btn-sm')}`)}
      </div>
      ${this.pills([['0°', 0], ['90°', 90], ['180°', 180], ['270°', 270]], null, 'set-rotation')}
      <div class="fields">
        ${this.materialField('Set wall material', 'multi', 'material', 'wall', '', false)}
      </div>
      <div class="action-row">
        ${this.button(`Duplicate ${ids.length}`, 'duplicate')}
        ${this.button(`Delete ${ids.length}`, 'delete', {}, 'btn btn-danger')}
      </div>
      <p class="hint">Hold Shift and drag in the view to marquee-select.</p>
    `;
  }

  // ------------------------------------------------------------- field access

  /** Resolves the object a `data-obj` field belongs to. */
  targetFor(obj) {
    const level = this.app.level;
    if (obj === 'level') return level;
    if (obj === 'defaults') return level.defaults;
    if (obj === 'spawn') return level.spawn;
    const id = [...this.app.editor.selectedIds][0];
    if (obj === 'opening') {
      const ref = LiminalOps.findOpening(level, id);
      return ref ? ref.opening : null;
    }
    if (obj === 'wall') return level.walls.find(w => w.id === id);
    if (obj === 'room') return level.rooms.find(r => r.id === id);
    if (obj === 'light') return level.ceiling_lights.find(l => l.id === id);
    if (obj === 'prop') return level.props.find(p => p.id === id);
    if (obj === 'patch') return level.floor_patches.find(p => p.id === id);
    if (obj === 'multi') return { ids: [...this.app.editor.selectedIds] };
    return null;
  }

  readField(obj, field) {
    const target = this.targetFor(obj);
    if (!target) return null;
    const wall = obj === 'wall' ? target : null;
    if (field === 'length' && wall) return LiminalGeometry.wallLength(wall);
    if (field === 'thickness' && wall) return LiminalGeometry.wallThickness(wall);
    if (field === 'fullHeight' && wall) return wall.height === null || wall.height === undefined;
    if (field === 'turned') return Math.round(target.rotation_degrees / 90) % 2 !== 0 ? 1 : 0;
    if (field.startsWith('face-') && wall) return wall.faces[field.slice(5)] || '';
    if (field.startsWith('size')) {
      const index = Number(field.slice(4));
      const size = target.size || this.app.propCatalog.get(target.model).size;
      return size[index];
    }
    return target[field];
  }

  /** Applies a field edit. `commit` decides whether it becomes an undo entry. */
  applyField(obj, field, value, commit, label) {
    const target = this.targetFor(obj);
    if (!target) return;

    if (obj === 'multi') {
      if (field === 'material') {
        for (const id of target.ids) {
          const wall = this.app.level.walls.find(w => w.id === id);
          if (wall) {
            wall.material = value || null;
            wall.faces = {};
          }
        }
        this.app.levelChanged();
        if (commit) this.app.commit('Set wall material');
      }
      return;
    }

    if (field === 'fullHeight') {
      target.height = value ? null : Math.max(0.1, LiminalGeometry.wallResolvedHeight(target, this.app.level.getCeilingHeight()));
    } else if (field === 'length') {
      const length = Math.max(0.05, Number(value) || 0.05);
      const axis = LiminalGeometry.wallAxis(target);
      if (axis === 'x') target.width = length; else target.depth = length;
      LiminalOps.clampWallOpenings(target);
    } else if (field === 'thickness') {
      const thickness = Math.max(0.05, Number(value) || 0.05);
      const axis = LiminalGeometry.wallAxis(target);
      if (axis === 'x') target.depth = thickness; else target.width = thickness;
    } else if (field === 'x' || field === 'z' || field === 'width' || field === 'depth') {
      target[field] = Number(value);
      if (target.openings) LiminalOps.clampWallOpenings(target);
    } else if (field === 'turned') {
      target.rotation_degrees = value ? 90 : 0;
    } else if (field.startsWith('face-')) {
      const face = field.slice(5);
      if (value && String(value).trim()) target.faces[face] = String(value).trim();
      else delete target.faces[face];
    } else if (field.startsWith('size')) {
      const index = Number(field.slice(4));
      const current = target.size || this.app.propCatalog.get(target.model).size.slice();
      const size = current.slice();
      size[index] = Math.max(0.01, Number(value) || 0.01);
      target.size = size;
    } else if (field === 'kind') {
      target.kind = value;
      const preset = LiminalGeometry.OPENING_KINDS[value];
      if (preset) {
        target.sill = preset.sill;
        target.height = preset.height;
        target.width = Math.min(preset.width, Math.max(0.2, Number(target.width) || preset.width));
      }
      this.app.editor.selectedIds = new Set([target.id]);
    } else if (field === 'material' && obj === 'wall') {
      target.material = value || null;
      target.faces = {};
    } else if ((field === 'material' || field === 'ceiling_material') && obj === 'room') {
      target[field] = value || null;
    } else {
      const numeric = typeof value === 'number' || (typeof value === 'string' && value.trim() !== '' && Number.isFinite(Number(value)) && typeof target[field] === 'number');
      target[field] = numeric ? Number(value) : value;
    }

    this.validateLive(target, obj);
    this.app.levelChanged();
    if (commit) this.app.commit(label || `Edit ${field}`);
  }

  /** Keeps openings inside their wall as the user types. */
  validateLive(target, obj) {
    if (obj !== 'opening') return;
    const ref = LiminalOps.findOpening(this.app.level, target.id);
    if (!ref) return;
    const length = LiminalGeometry.wallLength(ref.wall);
    const height = LiminalGeometry.wallResolvedHeight(ref.wall, this.app.level.getCeilingHeight());
    target.width = Math.max(0.2, Math.min(length, target.width));
    target.offset = Math.max(0, Math.min(length - target.width, target.offset));
    target.sill = Math.max(0, Math.min(Math.max(0, height - 0.2), target.sill));
    target.height = Math.max(0.2, Math.min(height - target.sill, target.height));
  }

  // ----------------------------------------------------------------- events

  bindEvents() {
    this.container.addEventListener('input', (e) => {
      const el = e.target;
      if (!el.dataset || !el.dataset.obj || !el.dataset.field) return;
      if (el.type === 'file') return;
      const value = el.type === 'checkbox' ? el.checked : el.type === 'number' || el.type === 'range' ? Number(el.value) : el.value;
      this.applyField(el.dataset.obj, el.dataset.field, value, false, el.dataset.label);
      if (el.dataset.obj === 'opening' && el.dataset.field === 'offset') {
        // Keep the slider and the number box in sync while dragging.
        for (const sibling of this.container.querySelectorAll(`[data-obj="opening"][data-field="offset"]`)) {
          if (sibling !== el && sibling !== document.activeElement) sibling.value = el.value;
        }
      }
    });

    this.container.addEventListener('change', (e) => {
      const el = e.target;
      if (el.dataset && el.dataset.obj && el.dataset.field) {
        const value = el.type === 'checkbox' ? el.checked : el.type === 'number' ? Number(el.value) : el.value;
        this.applyField(el.dataset.obj, el.dataset.field, value, true, el.dataset.label);
      }
    });

    this.container.addEventListener('click', (e) => {
      const button = e.target.closest('[data-action]');
      if (!button) return;
      this.handleAction(button.dataset.action, button.dataset);
    });
  }

  handleAction(action, data) {
    const app = this.app;
    const id = [...app.editor.selectedIds][0];

    switch (action) {
      case 'validate':
        app.showValidation();
        return;
      case 'duplicate':
        app.duplicateSelection();
        return;
      case 'delete':
        app.deleteSelection();
        return;
      case 'select-id':
        app.editor.select(data.id);
        return;
      case 'select-wall': {
        const ref = LiminalOps.findOpening(app.level, id);
        if (ref) app.editor.select(ref.wall.id);
        return;
      }
      case 'zoom-to': {
        const target = this.targetFor(data.obj);
        if (target) app.focusObject(target.id || data.id);
        return;
      }
      case 'set-height': {
        const room = this.targetFor('room');
        if (room) {
          room.height = Number(data.value);
          app.levelChanged();
          app.commit('Set ceiling height');
        }
        return;
      }
      case 'set-rotation': {
        for (const selectedId of app.editor.selectedIds) {
          const prop = app.level.props.find(p => p.id === selectedId);
          if (prop) prop.rotation_degrees = Number(data.value);
          const light = app.level.ceiling_lights.find(l => l.id === selectedId);
          if (light) light.rotation_degrees = Number(data.value);
        }
        app.levelChanged();
        app.commit('Rotate');
        return;
      }
      case 'set-yaw':
        app.level.spawn.yaw_degrees = Number(data.value);
        app.levelChanged();
        app.commit('Set spawn facing');
        return;
      case 'remove-opening':
        LiminalOps.removeOpening(app.level, data.id);
        app.editor.clearSelection();
        app.levelChanged();
        app.commit('Remove opening');
        return;
      case 'add-opening': {
        const wall = app.level.walls.find(w => w.id === id);
        if (!wall) return;
        const preset = LiminalOps.defaultOpening(data.kind);
        const opening = LiminalOps.addOpening(wall, {
          kind: data.kind,
          offset: Math.max(0, (LiminalGeometry.wallLength(wall) - preset.width) / 2),
          width: preset.width,
          height: preset.height,
          sill: preset.sill
        });
        if (opening) {
          app.editor.select(opening.id);
          app.commit(data.kind === 'door' ? 'Add doorway' : 'Add window');
        }
        return;
      }
      case 'walls-for-room': {
        const room = this.targetFor('room');
        if (room) app.addWallsAroundRoom(room);
        return;
      }
      case 'walls-for-rooms':
        app.addWallsAroundAllRooms();
        return;
      case 'add-room':
        app.addRoomAtCenter();
        return;
      case 'add-light':
        app.addLightAtCenter();
        return;
      case 'multi-move': {
        const dx = Number(this.container.querySelector('#multi-dx').value) || 0;
        const dz = Number(this.container.querySelector('#multi-dz').value) || 0;
        LiminalOps.moveObjects(app.level, [...app.editor.selectedIds], dx, dz);
        app.levelChanged();
        app.commit('Move selection');
        return;
      }
      case 'import-texture': {
        const input = this.container.querySelector('#texture-input');
        if (input) input.click();
        if (input && !input.dataset.bound) {
          input.dataset.bound = '1';
          input.addEventListener('change', () => {
            if (input.files && input.files[0]) app.importTexture(input.files[0]);
          });
        }
        return;
      }
      case 'remove-texture':
        app.removeTexture(data.id);
        return;
      default:
        return;
    }
  }
}
