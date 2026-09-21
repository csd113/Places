// app.js - Application shell for the Liminal level editor.
//
// Owns: the level instance, view modes (2D / 3D / Split), simple vs advanced
// complexity, the tool option bar, the prop browser, undo/redo and the status bar.
// All editing logic lives in ops.js; the 2D renderer, 3D viewport and inspector are
// views over the same level + selection.

const TOOL_HINTS = {
  select: 'Click to select · drag to move · drag the handles to resize · Shift+click to add',
  room: 'Drag a rectangle to create a room',
  wall: 'Drag to draw a wall (drag sideways or up/down for its direction)',
  door: 'Click a wall to cut a doorway, or drag along it to set the width',
  window: 'Click a wall to cut a window, or drag along it to set the width',
  light: 'Click to place a ceiling light, then drag to fine-tune it',
  prop: 'Pick a prop on the right, then click in the view to place it',
  spawn: 'Click to put the player start where you want it',
  patch: 'Drag a rectangle to paint a floor material patch'
};

const VIEW_MODES = ['2d', '3d', 'split'];
const DEFAULT_TOOL_OPTIONS = {
  wallThickness: 0.35,
  wallHeight: null,
  roomWalls: false,
  doorWidth: 1.0,
  doorHeight: 2.1,
  windowWidth: 1.4,
  windowHeight: 1.2,
  windowSill: 1.0,
  lightFixture: 'core:fluorescent_panel_01',
  lightBrightness: 1.0,
  propRotation: 0,
  spawnFacing: 0,
  patchMaterial: 'core:carpet_damp_01'
};

class App {
  constructor() {
    this.dom = {
      canvas2d: document.getElementById('canvas-2d'),
      canvas3d: document.getElementById('canvas-3d'),
      pane2d: document.getElementById('pane-2d'),
      pane3d: document.getElementById('pane-3d'),
      panes: document.getElementById('panes'),
      fallback3d: document.getElementById('pane-3d-fallback'),
      toolOptions: document.getElementById('tool-options'),
      inspector: document.getElementById('inspector'),
      propBrowser: document.getElementById('prop-browser'),
      propGrid: document.getElementById('prop-grid'),
      propSearch: document.getElementById('prop-search'),
      propCategories: document.getElementById('prop-categories'),
      statusHint: document.getElementById('status-hint'),
      statusMessage: document.getElementById('status-message'),
      statusCoords: document.getElementById('status-coords'),
      statusSelection: document.getElementById('status-selection'),
      zoomReadout: document.getElementById('zoom-readout')
    };

    // Persistent preferences
    this.viewMode = this.readPreference('view', '2d', VIEW_MODES);
    this.advanced = this.readPreference('advanced', 'false') === 'true';
    this.toolOptions = Object.assign({}, DEFAULT_TOOL_OPTIONS, this.readJSONPreference('toolOptions', {}));

    // Shared level + selection state
    this.propCatalog = LiminalProps.PropCatalog.builtin();
    this.propProxies = LiminalProps.PropProxies.empty();
    this.activePropModel = (this.propCatalog.list[0] || {}).id || 'core:crate';
    this.propCategory = 'All';
    this.propQuery = '';
    this.propBrowserDismissed = false;
    this.showCeilings = false; // false = auto-hide ceilings from above, true = pinned on

    this.level = this.createStarterLevel();

    this.levelRevision = 1;
    this.selectionRevision = 1;
    this.renderRequested = false;
    this.dirty = false;

    // Subsystems
    this.renderer = new Renderer(this.dom.canvas2d);
    this.history = new HistoryManager();
    this.editor = new Editor(this.dom.canvas2d, this);
    this.propertiesPanel = new PropertiesPanel(this.dom.inspector, this);
    this.io = new LevelIO(this);
    this.viewport3d = null;

    this.history.pushState(this.level, 'New level');
    this.history.onChange((canUndo, canRedo, label) => {
      const undo = document.getElementById('btn-undo');
      const redo = document.getElementById('btn-redo');
      if (undo) undo.disabled = !canUndo;
      if (redo) redo.disabled = !canRedo;
      this.lastAction = label;
    });

    this.bindTopbar();
    this.bindKeyboard();
    this.bindViewportControls();
    this.io.setupDragAndDrop();

    this.setAdvanced(this.advanced, { silent: true });
    this.setViewMode(this.viewMode, { silent: true });
    this.setTool('select');
    this.renderer.fitToGeometry(this.level);
    this.updateZoomReadout();
    this.updateDirtyIndicator();
    this.renderPropBrowser();
    this.updateStatus('Ready — drag a room, or open a level');

    this.loadPropCatalog();
    this.loadPropProxies();
    this.requestRender();
  }

  // ------------------------------------------------------------ preferences

  readPreference(key, fallback, allowed) {
    try {
      const value = localStorage.getItem(`liminal.${key}`);
      if (value === null) return fallback;
      if (allowed && !allowed.includes(value)) return fallback;
      return value;
    } catch (err) {
      return fallback;
    }
  }

  readJSONPreference(key, fallback) {
    try {
      const raw = localStorage.getItem(`liminal.${key}`);
      return raw ? JSON.parse(raw) : fallback;
    } catch (err) {
      return fallback;
    }
  }

  writePreference(key, value) {
    try {
      localStorage.setItem(`liminal.${key}`, String(value));
    } catch (err) {
      /* storage may be unavailable (private mode) - preferences are optional */
    }
  }

  // --------------------------------------------------------- starter level

  /** A small, immediately usable room: floor, ceiling, walls, a door, a window, props. */
  createStarterLevel() {
    const level = new Level({
      format_version: 1,
      id: 'custom_room_01',
      name: 'Custom Room',
      author: 'Creator',
      spawn: { x: 0, z: 0, yaw_degrees: 0 },
      room: { x: -6, z: -7, width: 12, depth: 14, height: 3.5 }
    });

    const rect = { x: -6, z: -7, width: 12, depth: 14 };
    LiminalOps.addWallsAroundRect(level, rect, { thickness: 0.35, height: 3.5 });

    const north = level.walls[0];   // north edge (min Z)
    const west = level.walls[2];    // west edge (min X)
    const east = level.walls[3];    // east edge (max X)

    LiminalOps.addOpening(north, { kind: 'door', offset: (12.7 - 1.2) / 2, width: 1.2, height: 2.1 });
    LiminalOps.addOpening(east, { kind: 'window', offset: 2.0, width: 1.8, height: 1.2, sill: 1.0 });
    LiminalOps.addOpening(east, { kind: 'window', offset: 7.0, width: 1.8, height: 1.2, sill: 1.0 });
    LiminalOps.addOpening(west, { kind: 'window', offset: 5.0, width: 1.4, height: 1.2, sill: 1.0 });

    LiminalOps.addLight(level, { x: -3, z: -3 });
    LiminalOps.addLight(level, { x: 3, z: -3 });
    LiminalOps.addLight(level, { x: 0, z: 3 });

    LiminalOps.addProp(level, { model: 'core:couch', x: -3.5, z: 4.2, rotation_degrees: 0 }, this.propCatalog);
    LiminalOps.addProp(level, { model: 'core:table', x: 0, z: 2.0, rotation_degrees: 90 }, this.propCatalog);
    LiminalOps.addProp(level, { model: 'core:cabinet', x: 5.0, z: -4.0, rotation_degrees: -90 }, this.propCatalog);
    LiminalOps.addProp(level, { model: 'core:plant', x: -5.1, z: -6.1 }, this.propCatalog);

    LiminalOps.setSpawn(level, { x: 0, z: 0, yaw_degrees: 0 });
    return level;
  }

  // ------------------------------------------------------------- lifecycle

  setLevel(level, actionLabel) {
    this.level = level;
    this.editor.selectedIds.clear();
    this.history.clear();
    this.history.pushState(level, actionLabel || 'Open level');
    this.levelRevision++;
    this.selectionRevision++;
    this.dirty = false;
    this.renderer.fitToGeometry(level);
    this.updateZoomReadout();
    this.propertiesPanel.render();
    if (this.viewport3d) this.viewport3d.frameAll();
    this.levelChanged();
    this.updateDirtyIndicator();
  }

  /** Call after any change to the level's data (structure or values). */
  levelChanged() {
    this.levelRevision++;
    if (this.viewport3d) this.viewport3d.markDirty();
    this.propertiesPanel.syncValues();
    this.updateSelectionStatus();
    this.requestRender();
  }

  /** Records one undo entry for a completed user action. */
  commit(label) {
    this.history.pushState(this.level, label);
    this.dirty = true;
    this.updateDirtyIndicator();
    this.requestRender();
  }

  updateDirtyIndicator() {
    document.title = `${this.dirty ? '• ' : ''}Liminal Level Editor — ${this.level.name || this.level.id}`;
  }

  requestRender() {
    if (this.renderRequested) return;
    this.renderRequested = true;
    requestAnimationFrame(() => {
      this.renderRequested = false;
      const show2d = this.viewMode !== '3d';
      const show3d = this.viewMode !== '2d';
      if (show2d) this.renderer.render(this.level, this.editor, { catalog: this.propCatalog });
      if (show3d && this.viewport3d) this.viewport3d.render();
    });
  }

  // ------------------------------------------------------------ view modes

  setViewMode(mode, options) {
    let next = VIEW_MODES.includes(mode) ? mode : '2d';
    // Split view needs room for two usable panes; fall back to 2D on small screens.
    if (next === 'split' && typeof window !== 'undefined') {
      const rect = this.dom.panes.getBoundingClientRect ? this.dom.panes.getBoundingClientRect().width : 0;
      const available = rect || Math.max(0, (Number(window.innerWidth) || 1200) - 96 - 312);
      if (available < 760) {
        next = '2d';
        this.updateStatus('Split view needs a wider window — showing 2D. Widen the window and pick Split again.');
      }
    }
    this.viewMode = next;
    this.writePreference('view', next);

    const show2d = next !== '3d';
    const show3d = next !== '2d';
    this.dom.pane2d.hidden = !show2d;
    this.dom.pane3d.hidden = !show3d;
    this.dom.panes.dataset.mode = next;

    for (const button of document.querySelectorAll('#view-switch .seg-btn')) {
      button.classList.toggle('active', button.dataset.view === next);
    }

    if (show3d) this.ensureViewport();
    // The panes changed size: re-measure both canvases.
    this.renderer.resize();
    if (this.viewport3d) this.viewport3d.resize();
    if (show3d && this.viewport3d && !options?.silent) this.viewport3d.frameAll();
    this.requestRender();
  }

  ensureViewport() {
    if (this.viewport3d) return;
    if (typeof Viewport3D === 'undefined') {
      this.dom.fallback3d.hidden = false;
      return;
    }
    const viewport = new Viewport3D(this.dom.canvas3d, this);
    if (!viewport.isSupported || !viewport.isSupported()) {
      this.dom.fallback3d.hidden = false;
      return;
    }
    this.viewport3d = viewport;
    this.dom.fallback3d.hidden = true;
    this.viewport3d.resize();
    this.viewport3d.setCeilingsMode(this.showCeilings ? 'on' : 'auto');
    this.viewport3d.frameAll();
  }

  setAdvanced(enabled, options) {
    this.advanced = !!enabled;
    this.writePreference('advanced', this.advanced);
    const checkbox = document.getElementById('chk-advanced');
    if (checkbox) checkbox.checked = this.advanced;
    document.querySelector('.app').classList.toggle('advanced', this.advanced);
    if (!this.advanced) {
      for (const details of document.querySelectorAll('details.advanced[open]')) details.open = false;
    }
    if (!options || !options.silent) {
      this.propertiesPanel.render();
      this.renderToolOptions();
      this.updateStatus(this.advanced
        ? 'Advanced mode: exact coordinates, identifiers and raw properties are shown'
        : 'Simple mode: only the common controls are shown');
    }
  }

  // ------------------------------------------------------------- selection

  onSelectionChanged() {
    this.selectionRevision++;
    if (this.viewport3d) this.viewport3d.markSelectionDirty();
    this.propertiesPanel.render();
    this.updateSelectionStatus();
    this.requestRender();
  }

  updateSelectionStatus() {
    const count = this.editor.selectedIds.size;
    if (count === 0) {
      this.dom.statusSelection.textContent = 'Nothing selected';
      return;
    }
    if (count === 1) {
      const id = [...this.editor.selectedIds][0];
      let label = '1 object';
      if (id === 'spawn') label = 'Player spawn';
      else if (this.level.walls.some(w => w.id === id)) label = 'Wall';
      else if (this.level.rooms.some(r => r.id === id)) label = 'Room';
      else if (this.level.ceiling_lights.some(l => l.id === id)) label = 'Light';
      else if (this.level.props.some(p => p.id === id)) label = 'Prop';
      else if (this.level.floor_patches.some(p => p.id === id)) label = 'Floor patch';
      else if (LiminalOps.findOpening(this.level, id)) label = 'Opening';
      this.dom.statusSelection.textContent = label;
      return;
    }
    this.dom.statusSelection.textContent = `${count} objects`;
  }

  updateCoords(x, z) {
    this.dom.statusCoords.textContent = `${x.toFixed(2)}, ${z.toFixed(2)}`;
  }

  updateStatus(message) {
    this.dom.statusMessage.textContent = message;
    if (this.statusTimeout) clearTimeout(this.statusTimeout);
    this.statusTimeout = setTimeout(() => {
      this.dom.statusMessage.textContent = '';
    }, 5000);
  }

  updateZoomReadout() {
    const percent = Math.round((this.renderer.zoom / 28) * 100);
    this.dom.zoomReadout.textContent = `${percent}%`;
  }

  onToolChanged(tool) {
    for (const button of document.querySelectorAll('.toolrail .tool[data-tool]')) {
      button.classList.toggle('active', button.dataset.tool === tool);
    }
    this.dom.statusHint.textContent = TOOL_HINTS[tool] || '';
    this.renderToolOptions();
    this.showPropBrowser(tool === 'prop');
    this.propertiesPanel.render();
  }

  setTool(tool) {
    this.editor.setTool(tool);
  }

  focusSelected() {
    const ids = [...this.editor.selectedIds];
    if (ids.length === 0) {
      if (this.viewMode !== '3d') {
        this.renderer.fitToGeometry(this.level);
        this.updateZoomReadout();
      }
      if (this.viewport3d && this.viewMode !== '2d') this.viewport3d.frameAll();
      return;
    }
    if (this.viewMode !== '3d') {
      const bounds = LiminalOps.objectBounds2D(this.level, ids[0], this.propCatalog);
      this.renderer.centerOn(bounds, ids.length === 1);
      this.updateZoomReadout();
    }
    if (this.viewport3d && this.viewMode !== '2d') this.viewport3d.focusSelected();
    this.updateStatus(ids.length === 1 ? 'Centred on the selection' : 'Centred on the selection');
  }

  focusObject(id) {
    this.editor.select(id);
    this.focusSelected();
  }

  // ------------------------------------------------------------ 3D intents

  viewportSelect(id, additive) {
    if (!id) {
      this.editor.clearSelection();
      return;
    }
    this.editor.select(id, !!additive);
    if (this.viewMode === 'split') this.requestRender();
  }

  viewportDragBegin() {
    this.drag3d = { ids: [...this.editor.selectedIds], initial: new Map(), totalX: 0, totalZ: 0 };
    for (const id of this.drag3d.ids) {
      const found = LiminalOps.findObject(this.level, id);
      if (found && found.object.x !== undefined) this.drag3d.initial.set(id, { x: found.object.x, z: found.object.z });
    }
  }

  /** The viewport reports incremental world-space steps; apply them from the snapshot. */
  viewportDragUpdate(stepX, stepZ) {
    if (!this.drag3d) return;
    this.drag3d.totalX += stepX;
    this.drag3d.totalZ += stepZ;
    for (const [id, initial] of this.drag3d.initial) {
      LiminalOps.moveObjectTo(
        this.level,
        id,
        Number((initial.x + this.drag3d.totalX).toFixed(4)),
        Number((initial.z + this.drag3d.totalZ).toFixed(4))
      );
    }
    this.levelChanged();
  }

  viewportDragEnd() {
    if (!this.drag3d) return;
    const moved = Math.abs(this.drag3d.totalX) > 1e-4 || Math.abs(this.drag3d.totalZ) > 1e-4;
    this.drag3d = null;
    if (moved) this.commit('Move');
  }

  // -------------------------------------------------------------- commands

  duplicateSelection() {
    const ids = [...this.editor.selectedIds];
    if (ids.length === 0) return;
    const newIds = LiminalOps.duplicateObjects(this.level, ids, { dx: 0.5, dz: 0.5 });
    this.editor.selectMany(newIds);
    this.levelChanged();
    this.commit('Duplicate');
    this.updateStatus(`Duplicated ${newIds.length} object${newIds.length === 1 ? '' : 's'}`);
  }

  deleteSelection() {
    const ids = [...this.editor.selectedIds];
    if (ids.length === 0) return;
    const result = LiminalOps.deleteObjects(this.level, ids);
    this.editor.clearSelection();
    this.levelChanged();
    this.commit('Delete');
    this.updateStatus(`Deleted ${result.deleted} object${result.deleted === 1 ? '' : 's'}${result.spawnReset ? ' (the player spawn stays, moved to 0,0)' : ''}`);
  }

  undo() {
    const previous = this.history.undo(this.level);
    if (!previous) return;
    this.level = previous;
    this.editor.clearSelection();
    this.levelChanged();
    this.updateDirtyIndicator();
    this.updateStatus(`Undo: ${this.history.lastAction}`);
  }

  redo() {
    const next = this.history.redo(this.level);
    if (!next) return;
    this.level = next;
    this.editor.clearSelection();
    this.levelChanged();
    this.updateDirtyIndicator();
    this.updateStatus(`Redo: ${this.history.lastAction}`);
  }

  rotateSelection(degrees) {
    const ids = [...this.editor.selectedIds];
    if (ids.length === 0) return;
    let rotated = 0;
    for (const id of ids) {
      const prop = this.level.props.find(p => p.id === id);
      if (prop) {
        prop.rotation_degrees = Number((((prop.rotation_degrees + degrees) % 360) + 360) % 360);
        rotated++;
      }
      const light = this.level.ceiling_lights.find(l => l.id === id);
      if (light) {
        light.rotation_degrees = Number((((light.rotation_degrees + degrees) % 360) + 360) % 360);
        rotated++;
      }
      if (id === 'spawn') {
        this.level.spawn.yaw_degrees = Number((((this.level.spawn.yaw_degrees + degrees) % 360) + 360) % 360);
        rotated++;
      }
    }
    if (rotated === 0) return;
    this.levelChanged();
    this.commit('Rotate');
  }

  addRoomAtCenter() {
    const center = this.viewCenter();
    const room = LiminalOps.createRoom(this.level, { x: center.x - 4, z: center.z - 4, width: 8, depth: 8, height: this.level.getCeilingHeight() });
    LiminalOps.addWallsAroundRect(this.level, { x: room.x, z: room.z, width: room.width, depth: room.depth }, { thickness: this.toolOptions.wallThickness });
    this.editor.select(room.id);
    this.levelChanged();
    this.commit('Add room');
    this.updateStatus('Room added — resize it with the handles or the inspector');
  }

  addLightAtCenter() {
    const center = this.viewCenter();
    const light = LiminalOps.addLight(this.level, { x: center.x, z: center.z });
    this.editor.select(light.id);
    this.levelChanged();
    this.commit('Add light');
  }

  addWallsAroundRoom(room) {
    const rect = { x: Math.min(room.x, room.x + room.width), z: Math.min(room.z, room.z + room.depth), width: Math.abs(room.width), depth: Math.abs(room.depth) };
    const created = LiminalOps.addWallsAroundRect(this.level, rect, { thickness: this.toolOptions.wallThickness || 0.35 });
    this.levelChanged();
    if (created.length > 0) this.commit('Add walls');
    this.updateStatus(created.length > 0
      ? `Added ${created.length} wall${created.length === 1 ? '' : 's'} around the room — add a doorway with the Door tool`
      : 'This room already has walls');
  }

  addWallsAroundAllRooms() {
    let created = 0;
    for (const room of this.level.rooms) {
      const rect = { x: Math.min(room.x, room.x + room.width), z: Math.min(room.z, room.z + room.depth), width: Math.abs(room.width), depth: Math.abs(room.depth) };
      created += LiminalOps.addWallsAroundRect(this.level, rect, { thickness: this.toolOptions.wallThickness || 0.35 }).length;
    }
    this.levelChanged();
    if (created > 0) this.commit('Add walls');
    this.updateStatus(created > 0 ? `Added ${created} walls` : 'Every room already has walls');
  }

  viewCenter() {
    if (this.viewMode !== '2d' && this.editor.selectedIds.size > 0) {
      const bounds = LiminalOps.objectBounds2D(this.level, [...this.editor.selectedIds][0], this.propCatalog);
      if (bounds) return { x: Math.round(bounds.x + bounds.width / 2), z: Math.round(bounds.z + bounds.depth / 2) };
    }
    return { x: Math.round(this.renderer.cameraX / 0.5) * 0.5, z: Math.round(this.renderer.cameraZ / 0.5) * 0.5 };
  }

  async importTexture(file) {
    const dataUrl = await new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = (e) => resolve(e.target.result);
      reader.onerror = reject;
      reader.readAsDataURL(file);
    });
    const dimensions = await this.io.getImageDimensions(dataUrl);
    if (dimensions.width > 1024 || dimensions.height > 1024) {
      this.updateStatus(`Warning: ${dimensions.width}×${dimensions.height} exceeds the game's 1024×1024 texture limit`);
    }
    const canvas = document.createElement('canvas');
    canvas.width = dimensions.width;
    canvas.height = dimensions.height;
    const ctx = canvas.getContext('2d');
    const image = new Image();
    await new Promise((resolve) => {
      image.onload = resolve;
      image.onerror = resolve;
      image.src = dataUrl;
    });
    ctx.drawImage(image, 0, 0);

    const cleanName = file.name.replace(/\.[^/.]+$/, '').replace(/[^a-zA-Z0-9_-]/g, '_').toLowerCase();
    const materialId = `pack:${cleanName}`;
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, 'image/png'));
    const bytes = new Uint8Array(await blob.arrayBuffer());

    this.level.custom_textures[materialId] = {
      filename: `textures/${cleanName}.png`,
      dataUrl: canvas.toDataURL('image/png'),
      width: dimensions.width,
      height: dimensions.height,
      bytes
    };
    this.levelChanged();
    this.commit(`Import texture ${materialId}`);
    this.updateStatus(`Imported ${materialId} (${dimensions.width}×${dimensions.height})`);
  }

  removeTexture(materialId) {
    delete this.level.custom_textures[materialId];
    this.levelChanged();
    this.commit(`Remove texture ${materialId}`);
  }

  // ------------------------------------------------------------ validation

  showValidation(auto) {
    const result = validateLevel(this.level);
    const stats = LiminalGeometry.levelStats(this.level);
    const body = document.getElementById('validation-results');
    const modal = document.getElementById('validation-modal');

    let html = '';
    if (result.errors.length === 0 && result.warnings.length === 0) {
      html += `<div class="banner ok"><strong>✓ This level is ready to play</strong>
        <p>Schema, sizes, spawn and materials all check out. The game can load it as-is.</p></div>`;
    } else if (result.errors.length === 0) {
      html += `<div class="banner warn"><strong>✓ Playable, with ${result.warnings.length} warning${result.warnings.length === 1 ? '' : 's'}</strong>
        <p>The game will load this level. Warnings are usually missing textures or unusual sizes.</p></div>`;
    } else {
      html += `<div class="banner err"><strong>${result.errors.length} problem${result.errors.length === 1 ? '' : 's'} to fix</strong>
        <p>liminal-rust will refuse to load the level until these are resolved.</p></div>`;
    }

    if (result.errors.length > 0) {
      html += `<h4>Problems</h4><ul class="issue-list">${result.errors.map(e => `<li>${this.escape(e)}</li>`).join('')}</ul>`;
    }
    if (result.warnings.length > 0) {
      html += `<h4>Warnings</h4><ul class="issue-list">${result.warnings.map(w => `<li class="warn">${this.escape(w)}</li>`).join('')}</ul>`;
    }

    html += `<h4>What's in this level</h4>
      <div class="stat-grid">
        <span>Rooms</span><strong>${stats.rooms}</strong>
        <span>Walls</span><strong>${stats.walls}</strong>
        <span>Doors &amp; windows</span><strong>${stats.openings}</strong>
        <span>Ceiling lights</span><strong>${stats.lights}</strong>
        <span>Props</span><strong>${stats.props}</strong>
        <span>Imported textures</span><strong>${Object.keys(this.level.custom_textures || {}).length}</strong>
      </div>`;

    body.innerHTML = html;
    this.openModal(modal);
    if (!auto && result.errors.length === 0) this.updateStatus('Level check passed');
    return result;
  }

  showPlay() {
    const result = validateLevel(this.level);
    const body = document.getElementById('play-results');
    const filename = `${(this.level.id || 'level').replace(/[^a-zA-Z0-9_-]/g, '_')}.json`;

    const blocked = result.errors.length > 0;
    let html = blocked
      ? `<div class="banner err"><strong>Fix the ${result.errors.length} problem${result.errors.length === 1 ? '' : 's'} first</strong>
          <p>The game will not load this level yet. Open “Validate” to see the details.</p></div>
         <p><button class="btn" id="play-validate">Show the problems</button></p>`
      : `<div class="banner ok"><strong>Level ready</strong>
          <p>Save the file, drop it in the game's import folder, then load it from the in-game Level Select screen.</p></div>
         <ol class="steps">
           <li>Click <strong>Save level file</strong> below — it writes <code>${filename}</code>.</li>
           <li>Move that file into <code>apps/liminal-rust/import/</code> (create the folder if it does not exist).</li>
           <li>Start the game, choose <strong>Level Select → Load / Import</strong>, and pick this level.</li>
           <li>Use the same flow after every change: save again, then re-import in the game.</li>
         </ol>
         <p><button class="btn btn-primary" id="play-save">Save level file</button>
            <button class="btn" id="play-copy">Copy level JSON</button></p>`;

    body.innerHTML = html;
    this.openModal(document.getElementById('play-modal'));

    const saveButton = document.getElementById('play-save');
    if (saveButton) saveButton.addEventListener('click', async () => {
      await this.io.saveLevelFile(this.level);
      this.updateStatus('Level saved — copy it into the game\'s import folder');
    });
    const copyButton = document.getElementById('play-copy');
    if (copyButton) copyButton.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(this.io.levelJSON(this.level));
        this.updateStatus('Level JSON copied to the clipboard');
      } catch (err) {
        this.updateStatus('Clipboard unavailable — use Save level file instead');
      }
    });
    const validateButton = document.getElementById('play-validate');
    if (validateButton) validateButton.addEventListener('click', () => {
      document.getElementById('play-modal').hidden = true;
      this.showValidation();
    });
  }

  escape(value) {
    return String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  openModal(modal) {
    if (modal) modal.hidden = false;
  }

  closeModals() {
    for (const modal of document.querySelectorAll('.modal')) modal.hidden = true;
  }

  // ------------------------------------------------------- tool options bar

  renderToolOptions() {
    const tool = this.editor.currentTool;
    const options = this.toolOptions;
    const parts = [];
    const numberOpt = (key, label, step, min, max) => `<span class="opt"><span class="opt-label">${label}</span>
      <input type="number" data-option="${key}" step="${step}" ${min !== undefined ? `min="${min}"` : ''} ${max !== undefined ? `max="${max}"` : ''} value="${options[key]}"></span>`;

    switch (tool) {
      case 'select':
        parts.push('<span class="opt-note">Click to select · drag to move · drag the white handles to resize · Shift+click adds to the selection</span>');
        break;
      case 'room':
        parts.push(`<span class="opt"><label class="mini-switch"><input type="checkbox" data-option="roomWalls" ${options.roomWalls ? 'checked' : ''}><span>Add walls around each room</span></label></span>`);
        parts.push(numberOpt('wallThickness', 'Wall thickness', 0.05, 0.05, 2));
        break;
      case 'wall':
        parts.push(numberOpt('wallThickness', 'Thickness', 0.05, 0.05, 2));
        parts.push(`<span class="opt"><span class="opt-label">Height</span>
          <select data-option="wallHeight">
            <option value="full"${options.wallHeight === null ? ' selected' : ''}>Full ceiling</option>
            <option value="1.0"${options.wallHeight === 1.0 ? ' selected' : ''}>1.0 m (sill)</option>
            <option value="1.3"${options.wallHeight === 1.3 ? ' selected' : ''}>1.3 m</option>
            <option value="2.1"${options.wallHeight === 2.1 ? ' selected' : ''}>2.1 m (door height)</option>
            <option value="2.5"${options.wallHeight === 2.5 ? ' selected' : ''}>2.5 m</option>
          </select></span>`);
        break;
      case 'door':
        parts.push(numberOpt('doorWidth', 'Width', 0.1, 0.2, 10));
        parts.push(numberOpt('doorHeight', 'Height', 0.1, 0.2, 10));
        parts.push('<span class="opt-note">Drag along the wall to size the doorway, or click for the default width.</span>');
        break;
      case 'window':
        parts.push(numberOpt('windowWidth', 'Width', 0.1, 0.2, 10));
        parts.push(numberOpt('windowHeight', 'Height', 0.1, 0.2, 10));
        parts.push(numberOpt('windowSill', 'Sill', 0.1, 0, 10));
        break;
      case 'light':
        parts.push(`<span class="opt"><span class="opt-label">Fixture</span>
          <select data-option="lightFixture">
            <option value="core:fluorescent_panel_01"${options.lightFixture === 'core:fluorescent_panel_01' ? ' selected' : ''}>Fluorescent panel</option>
          </select></span>`);
        parts.push(numberOpt('lightBrightness', 'Brightness', 0.1, 0.1, 5));
        break;
      case 'prop':
        parts.push(`<span class="opt"><span class="opt-label">Rotation</span>
          <select data-option="propRotation">
            ${[0, 90, 180, 270].map(angle => `<option value="${angle}"${Number(options.propRotation) === angle ? ' selected' : ''}>${angle}°</option>`).join('')}
          </select></span>`);
        parts.push('<span class="opt-note">Click a prop on the right to choose it, then click in the view.</span>');
        break;
      case 'spawn':
        parts.push(`<span class="opt"><span class="opt-label">Facing</span>
          <select data-option="spawnFacing">
            ${[['North', 0], ['East', 90], ['South', 180], ['West', 270]].map(([label, value]) =>
              `<option value="${value}"${Number(options.spawnFacing) === value ? ' selected' : ''}>${label}</option>`).join('')}
          </select></span>`);
        break;
      case 'patch':
        parts.push(`<span class="opt"><span class="opt-label">Material</span>
          <select data-option="patchMaterial">
            <option value="core:carpet_damp_01"${options.patchMaterial === 'core:carpet_damp_01' ? ' selected' : ''}>Damp carpet</option>
            <option value="core:carpet_beige_01"${options.patchMaterial === 'core:carpet_beige_01' ? ' selected' : ''}>Beige carpet</option>
          </select></span>`);
        parts.push('<span class="opt-note">Paint a floor patch (damp carpet, stains, rugs).</span>');
        break;
      default:
        break;
    }

    this.dom.toolOptions.innerHTML = parts.join('');
  }

  bindToolOptions() {
    this.dom.toolOptions.addEventListener('input', (e) => this.applyToolOption(e));
    this.dom.toolOptions.addEventListener('change', (e) => this.applyToolOption(e));
  }

  applyToolOption(event) {
    const el = event.target;
    const key = el.dataset ? el.dataset.option : null;
    if (!key) return;
    let value;
    if (el.type === 'checkbox') value = el.checked;
    else if (el.tagName === 'SELECT' && key === 'wallHeight') value = el.value === 'full' ? null : Number(el.value);
    else if (el.type === 'number') value = Number(el.value);
    else if (el.tagName === 'SELECT' && (key === 'propRotation' || key === 'spawnFacing')) value = Number(el.value);
    else value = el.value;

    this.toolOptions[key] = value;
    this.writePreference('toolOptions', JSON.stringify(this.toolOptions));
    if (key === 'spawnFacing') {
      this.updateStatus('New spawns will face ' + el.options[el.selectedIndex].text);
    }
  }

  // ---------------------------------------------------------- prop browser

  showPropBrowser(visible) {
    // The browser follows the Prop tool; the close button hides it until the tool is
    // picked again.
    if (visible) this.propBrowserDismissed = false;
    this.dom.propBrowser.hidden = !(visible && !this.propBrowserDismissed);
  }

  /**
   * Thumbnail URL for a catalogue entry. `tools/props/build.py --thumbs` writes
   * `assets/thumbs/<short-name>.png` (the id after `core:`); entries with no
   * model, or a missing image, simply keep the colour swatch behind.
   */
  propThumbUrl(entry) {
    if (!entry || !entry.model) return null;
    const short = String(entry.id || '').split(':').pop();
    if (!short) return null;
    return 'assets/thumbs/' + encodeURIComponent(short) + '.png';
  }

  renderPropBrowser() {
    const catalog = this.propCatalog;
    const categories = ['All'].concat(catalog.categories());
    this.dom.propCategories.innerHTML = categories.map(category =>
      `<button class="chip ${category === this.propCategory ? 'active' : ''}" data-category="${this.escape(category)}">${this.escape(category)}</button>`
    ).join('');

    const entries = catalog.search(this.propQuery, this.propCategory);
    this.dom.propGrid.innerHTML = entries.length === 0
      ? '<p class="hint">No props match. Add entries to <code>assets/props/props.json</code> — they appear here automatically.</p>'
      : entries.map(entry => {
        const thumb = this.propThumbUrl(entry);
        const image = thumb
          ? `<img src="${thumb}" alt="" loading="lazy" onerror="this.style.display='none'">`
          : '';
        // Real asset budgets from prop_proxies.json, so the browser shows what
        // each prop costs on the PocketCHIP instead of a hand-written guess.
        const proxy = this.propProxies && this.propProxies.get(entry.id);
        const budget = proxy && proxy.triangles
          ? ` · ${proxy.triangles} tris`
          : '';
        return `
        <button class="prop-card ${entry.id === this.activePropModel ? 'active' : ''}" data-model="${this.escape(entry.id)}"
          title="${this.escape(entry.name)} · ${entry.size.map(v => v.toFixed(2)).join(' × ')} m${budget}">
          <span class="prop-thumb" style="--prop-color: rgb(${entry.color.map(v => Math.round(v * 255)).join(',')})">${image}</span>
          <span class="prop-name">${this.escape(entry.name)}</span>
          <span class="prop-meta">${this.escape(entry.category)}</span>
        </button>`;
      }).join('');
  }

  bindPropBrowser() {
    this.dom.propSearch.addEventListener('input', () => {
      this.propQuery = this.dom.propSearch.value;
      this.renderPropBrowser();
    });
    this.dom.propCategories.addEventListener('click', (e) => {
      const chip = e.target.closest('[data-category]');
      if (!chip) return;
      this.propCategory = chip.dataset.category;
      this.renderPropBrowser();
    });
    this.dom.propGrid.addEventListener('click', (e) => {
      const card = e.target.closest('[data-model]');
      if (!card) return;
      this.activePropModel = card.dataset.model;
      this.renderPropBrowser();
      const entry = this.propCatalog.get(this.activePropModel);
      this.updateStatus(`${entry.name} selected — click in the view to place it`);
      if (this.editor.currentTool !== 'prop') this.setTool('prop');
    });
    document.getElementById('btn-prop-browser-close').addEventListener('click', () => {
      this.propBrowserDismissed = true;
      this.dom.propBrowser.hidden = true;
    });
  }

  async loadPropCatalog() {
    try {
      const catalog = await LiminalProps.loadPropCatalog();
      if (catalog && catalog.size > 0) {
        this.propCatalog = catalog;
        const stillExists = catalog.has(this.activePropModel);
        this.activePropModel = stillExists ? this.activePropModel : ((catalog.list[0] || {}).id || this.activePropModel);
        this.levelRevision++;
        if (this.viewport3d) this.viewport3d.markDirty();
        this.renderPropBrowser();
        this.propertiesPanel.render();
        this.requestRender();
      }
    } catch (err) {
      // The built-in catalogue is always available; nothing to do.
    }
  }

  async loadPropProxies() {
    try {
      const proxies = await LiminalProps.loadPropProxies();
      if (proxies && proxies.size > 0) {
        this.propProxies = proxies;
        // The 3D mesh substitutes real part geometry, so it must be rebuilt,
        // and the browser refreshes to show the real triangle budgets.
        this.levelRevision++;
        if (this.viewport3d) this.viewport3d.markDirty();
        this.renderPropBrowser();
        this.requestRender();
      }
    } catch (err) {
      // No proxies: props keep the catalogue-box fallback.
    }
  }

  // -------------------------------------------------------------- viewport

  bindViewportControls() {
    const xray = document.getElementById('btn-3d-xray');
    const ceilings = document.getElementById('btn-3d-ceilings');
    const focus = document.getElementById('btn-3d-focus');
    const reset = document.getElementById('btn-3d-reset');

    xray.addEventListener('click', () => {
      if (!this.viewport3d) return;
      const next = !this.viewport3d.isXray();
      this.viewport3d.setXray(next);
      xray.classList.toggle('active', next);
      this.updateStatus(next ? 'X-ray: walls are see-through' : 'Walls are solid again');
      this.requestRender();
    });
    ceilings.addEventListener('click', () => {
      if (!this.viewport3d) return;
      // Two states only: auto-hide while looking down from above (default) and
      // always visible.
      this.showCeilings = !this.showCeilings;
      this.viewport3d.setCeilingsMode(this.showCeilings ? 'on' : 'auto');
      ceilings.classList.toggle('active', this.showCeilings);
      ceilings.title = this.showCeilings ? 'Ceilings are always drawn — click to auto-hide them from above' : 'Show ceilings';
      this.updateStatus(this.showCeilings ? 'Ceilings always visible' : 'Ceilings hidden while the camera is above them');
      this.requestRender();
    });
    focus.addEventListener('click', () => this.focusSelected());
    reset.addEventListener('click', () => {
      if (this.viewport3d) this.viewport3d.resetCamera();
      this.requestRender();
    });
  }

  // ---------------------------------------------------------------- topbar

  bindTopbar() {
    document.getElementById('btn-new').addEventListener('click', () => {
      if (this.dirty && !confirm('Start a new level? Unsaved changes will be lost.')) return;
      this.setLevel(this.createStarterLevel(), 'New level');
      this.updateStatus('New level started — a starter room with a door and windows');
    });

    const fileInput = document.getElementById('file-import-input');
    document.getElementById('btn-open').addEventListener('click', () => fileInput.click());
    fileInput.addEventListener('change', () => {
      if (fileInput.files.length > 0) {
        this.io.importFile(fileInput.files[0]);
        fileInput.value = '';
      }
    });

    document.getElementById('btn-save').addEventListener('click', () => {
      this.io.exportJSON(this.level);
      this.dirty = false;
      this.updateDirtyIndicator();
    });
    document.getElementById('btn-export-pack').addEventListener('click', () => this.io.exportZIP(this.level));
    document.getElementById('btn-validate').addEventListener('click', () => this.showValidation());
    document.getElementById('btn-play').addEventListener('click', () => this.showPlay());

    for (const button of document.querySelectorAll('#view-switch .seg-btn')) {
      button.addEventListener('click', () => this.setViewMode(button.dataset.view));
    }

    document.getElementById('chk-advanced').addEventListener('change', (e) => this.setAdvanced(e.target.checked));

    document.getElementById('btn-undo').addEventListener('click', () => this.undo());
    document.getElementById('btn-redo').addEventListener('click', () => this.redo());
    document.getElementById('btn-duplicate').addEventListener('click', () => this.duplicateSelection());
    document.getElementById('btn-delete').addEventListener('click', () => this.deleteSelection());

    for (const button of document.querySelectorAll('.toolrail .tool[data-tool]')) {
      button.addEventListener('click', () => this.setTool(button.dataset.tool));
    }

    const snap = document.getElementById('chk-snap');
    snap.addEventListener('change', () => {
      this.editor.snapEnabled = snap.checked;
      this.updateStatus(snap.checked ? `Snap: ${this.editor.snapStep} m` : 'Snap off');
    });
    const snapStep = document.getElementById('select-snap');
    snapStep.addEventListener('change', () => {
      this.editor.snapStep = Number(snapStep.value);
      this.updateStatus(`Snap: ${this.editor.snapStep} m`);
    });
    const heights = document.getElementById('chk-heights');
    heights.addEventListener('change', () => {
      this.renderer.showHeightBadges = heights.checked;
      this.requestRender();
    });

    document.getElementById('btn-fit').addEventListener('click', () => {
      this.renderer.fitToGeometry(this.level);
      this.updateZoomReadout();
      this.requestRender();
    });

    document.getElementById('btn-help').addEventListener('click', () => this.openModal(document.getElementById('help-modal')));
    for (const button of document.querySelectorAll('.modal-close')) {
      button.addEventListener('click', () => this.closeModals());
    }
    for (const modal of document.querySelectorAll('.modal')) {
      modal.addEventListener('mousedown', (e) => {
        if (e.target === modal) this.closeModals();
      });
    }

    this.bindToolOptions();
    this.bindPropBrowser();

    // The 3D viewport owns WASD/QE/F/R while the pointer is over its pane; every
    // other shortcut still belongs to the editor.
    this.pointerIn3d = false;
    this.dom.pane3d.addEventListener('mouseenter', () => { this.pointerIn3d = true; });
    this.dom.pane3d.addEventListener('mouseleave', () => { this.pointerIn3d = false; });

    window.addEventListener('resize', () => {
      this.renderer.resize();
      if (this.viewport3d) this.viewport3d.resize();
      this.requestRender();
    });
  }

  bindKeyboard() {
    window.addEventListener('keydown', (e) => {
      const tag = (e.target || {}).tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      // The 3D viewport owns its movement keys while the pointer is over it (or the
      // canvas has focus), so both systems never act on the same key press.
      const inViewport = (e.target && e.target.id === 'canvas-3d') || this.pointerIn3d;
      const ctrl = e.ctrlKey || e.metaKey;

      if (ctrl && (e.key === 'z' || e.key === 'Z')) {
        e.preventDefault();
        if (e.shiftKey) this.redo(); else this.undo();
        return;
      }
      if (ctrl && e.key === 'y') { e.preventDefault(); this.redo(); return; }
      if (ctrl && e.key === 'd') { e.preventDefault(); this.duplicateSelection(); return; }
      if (ctrl && e.key === 'a') { e.preventDefault(); this.editor.selectAll(); return; }
      if (ctrl && e.key === 's') {
        e.preventDefault();
        this.io.exportJSON(this.level);
        this.dirty = false;
        this.updateDirtyIndicator();
        return;
      }
      if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); this.deleteSelection(); return; }
      if (e.key === 'Escape') {
        this.closeModals();
        this.editor.cancelOperation();
        this.setTool('select');
        return;
      }
      if (e.key === '?') { this.openModal(document.getElementById('help-modal')); return; }
      if (e.key === '1') { this.setViewMode('2d'); return; }
      if (e.key === '2') { this.setViewMode('3d'); return; }
      if (e.key === '3') { this.setViewMode('split'); return; }

      if (ctrl) return;
      if (inViewport && ['w', 'a', 's', 'd', 'q', 'e', 'f', 'r', ' '].includes(e.key.toLowerCase())) return;

      switch (e.key.toLowerCase()) {
        case 'v': this.setTool('select'); break;
        case 'r': this.setTool('room'); break;
        case 'w': this.setTool('wall'); break;
        case 'd': this.setTool('door'); break;
        case 'n': this.setTool('window'); break;
        case 'l': this.setTool('light'); break;
        case 'p': this.setTool('prop'); break;
        case 'm': this.setTool('spawn'); break;
        case 't':
          if (this.advanced) this.setTool('patch');
          else this.updateStatus('The floor patch tool is an advanced tool — turn on Advanced to use it');
          break;
        case 'f': this.focusSelected(); break;
        case 'g': {
          const snap = document.getElementById('chk-snap');
          snap.checked = !snap.checked;
          snap.dispatchEvent(new Event('change'));
          break;
        }
        case 'q': this.rotateSelection(-15); break;
        case 'e': this.rotateSelection(15); break;
        default: break;
      }
    });

    window.addEventListener('beforeunload', (e) => {
      if (!this.dirty) return undefined;
      e.preventDefault();
      e.returnValue = '';
      return '';
    });
  }
}

window.addEventListener('DOMContentLoaded', () => {
  window.app = new App();
});
