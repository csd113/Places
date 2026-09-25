// editor.js - Canvas interaction for the 2D plan view.
//
// This module only translates input into `PlacesOps` calls and preview state; all
// level mutations, geometry and validation live in ops.js / model.js / geometry.js so
// the 3D viewport, the inspector and the tests share exactly one implementation.
//
// One user-visible action becomes exactly one history entry: nothing is recorded
// while a drag is in flight, and `app.commit(label)` runs once on release.

class Editor {
  constructor(canvas, app) {
    this.canvas = canvas;
    this.app = app;
    this.renderer = app.renderer;

    this.currentTool = 'select';
    this.selectedIds = new Set();

    this.snapEnabled = true;
    this.snapStep = 0.5;

    // Interaction state
    this.isPanning = false;
    this.panStart = { x: 0, y: 0 };
    this.cameraStart = { x: 0, z: 0 };
    this.spacePressed = false;

    this.dragMode = null;         // pan | move | resize | marquee | draw | opening | place
    this.dragStart = { x: 0, y: 0 };
    this.dragWorldStart = { x: 0, z: 0 };
    this.dragInitial = new Map(); // id -> {x, z, offset}
    this.activeHandle = null;
    this.resizeTarget = null;
    this.resizeInitial = null;

    this.preview = null;          // {type:'rect'|'opening', ...}
    this.marqueeBox = null;
    this.hoverOpeningId = null;
    this.activeOpening = null;    // {wall, kind, startOffset}
    this.placedId = null;
    this.wasDragged = false;

    this.bindEvents();
  }

  // ------------------------------------------------------------------ tools

  setTool(tool) {
    this.currentTool = tool;
    this.dragMode = null;
    this.preview = null;
    this.marqueeBox = null;
    this.hoverOpeningId = null;
    this.activeOpening = null;
    this.updateCursor();
    this.app.onToolChanged(tool);
    this.app.requestRender();
  }

  snap(value) {
    if (!this.snapEnabled || this.snapStep <= 0) return value;
    return Math.round(value / this.snapStep) * this.snapStep;
  }

  snapPoint(point) {
    return { x: this.snap(point.x), z: this.snap(point.z) };
  }

  // -------------------------------------------------------------- selection

  select(id, additive = false) {
    if (!additive) this.selectedIds.clear();
    if (id) {
      if (additive && this.selectedIds.has(id)) this.selectedIds.delete(id);
      else this.selectedIds.add(id);
    }
    this.app.onSelectionChanged();
    this.app.requestRender();
  }

  selectMany(ids, additive = false) {
    if (!additive) this.selectedIds.clear();
    for (const id of ids) this.selectedIds.add(id);
    this.app.onSelectionChanged();
    this.app.requestRender();
  }

  clearSelection() {
    if (this.selectedIds.size === 0) return;
    this.selectedIds.clear();
    this.app.onSelectionChanged();
    this.app.requestRender();
  }

  selectAll() {
    const level = this.app.level;
    const ids = [];
    for (const r of level.rooms) ids.push(r.id);
    for (const w of level.walls) ids.push(w.id);
    for (const l of level.ceiling_lights) ids.push(l.id);
    for (const p of level.props) ids.push(p.id);
    if (level.spawn) ids.push('spawn');
    this.selectMany(ids);
    this.app.updateStatus(`Selected ${ids.length} object${ids.length === 1 ? '' : 's'}`);
  }

  // ------------------------------------------------------------------ input

  bindEvents() {
    const c = this.canvas;
    c.addEventListener('mousedown', (e) => this.onMouseDown(e));
    window.addEventListener('mousemove', (e) => this.onMouseMove(e));
    window.addEventListener('mouseup', (e) => this.onMouseUp(e));
    c.addEventListener('wheel', (e) => this.onWheel(e), { passive: false });
    c.addEventListener('contextmenu', (e) => e.preventDefault());
    c.addEventListener('mouseleave', () => {
      if (this.preview && (this.currentTool === 'door' || this.currentTool === 'window')) {
        this.preview = null;
        this.app.requestRender();
      }
    });

    window.addEventListener('keydown', (e) => {
      if (e.code === 'Space' && !this.spacePressed && !this.isEditingInput(e)) {
        this.spacePressed = true;
        this.updateCursor();
        e.preventDefault();
      }
    });
    window.addEventListener('keyup', (e) => {
      if (e.code === 'Space') {
        this.spacePressed = false;
        this.updateCursor();
      }
    });
  }

  isEditingInput(e) {
    const tag = (e.target || {}).tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT';
  }

  updateCursor(handle = null) {
    if (this.isPanning || this.spacePressed) {
      this.canvas.style.cursor = 'grab';
      return;
    }
    if (handle) {
      const map = {
        nw: 'nwse-resize', se: 'nwse-resize', ne: 'nesw-resize', sw: 'nesw-resize',
        n: 'ns-resize', s: 'ns-resize', e: 'ew-resize', w: 'ew-resize'
      };
      this.canvas.style.cursor = map[handle] || 'default';
      return;
    }
    switch (this.currentTool) {
      case 'select':
        this.canvas.style.cursor = 'default';
        break;
      case 'room':
      case 'wall':
      case 'patch':
        this.canvas.style.cursor = 'crosshair';
        break;
      case 'door':
      case 'window':
        this.canvas.style.cursor = 'pointer';
        break;
      default:
        this.canvas.style.cursor = 'copy';
    }
  }

  getCanvasPoint(e) {
    const rect = this.canvas.getBoundingClientRect();
    return { x: e.clientX - rect.left, y: e.clientY - rect.top };
  }

  onWheel(e) {
    e.preventDefault();
    const pt = this.getCanvasPoint(e);
    const before = this.renderer.screenToWorld(pt.x, pt.y);
    const factor = e.deltaY < 0 ? 1.15 : 1 / 1.15;
    const zoom = Math.max(this.renderer.minZoom, Math.min(this.renderer.maxZoom, this.renderer.zoom * factor));
    if (zoom === this.renderer.zoom) return;
    this.renderer.zoom = zoom;
    const after = this.renderer.screenToWorld(pt.x, pt.y);
    this.renderer.cameraX += before.x - after.x;
    this.renderer.cameraZ += before.z - after.z;
    this.app.updateZoomReadout();
    this.app.requestRender();
  }

  onMouseDown(e) {
    const pt = this.getCanvasPoint(e);
    const world = this.renderer.screenToWorld(pt.x, pt.y);
    const snapped = this.snapPoint(world);

    if (e.button === 1 || e.button === 2 || (e.button === 0 && this.spacePressed)) {
      this.isPanning = true;
      this.panStart = { x: e.clientX, y: e.clientY };
      this.cameraStart = { x: this.renderer.cameraX, z: this.renderer.cameraZ };
      this.updateCursor();
      return;
    }
    if (e.button !== 0) return;

    this.wasDragged = false;
    this.dragStart = { x: pt.x, y: pt.y };

    switch (this.currentTool) {
      case 'select': return this.beginSelectDrag(pt, snapped, e.shiftKey);
      case 'room':
      case 'wall':
      case 'patch':
        this.dragMode = 'draw';
        this.preview = { type: 'rect', tool: this.currentTool, start: snapped, current: snapped };
        this.app.requestRender();
        return;
      case 'door':
      case 'window':
        return this.beginOpeningDrag(snapped);
      case 'light':
        return this.placeAndDrag(() => {
          const options = this.app.toolOptions;
          const light = PlacesOps.addLight(this.app.level, {
            x: snapped.x,
            z: snapped.z,
            fixture: options.lightFixture,
            brightness: options.lightBrightness
          });
          return light.id;
        }, 'Add light');
      case 'prop':
        return this.placeAndDrag(() => {
          const model = this.app.activePropModel;
          if (!model) {
            this.app.updateStatus('Pick a prop in the Prop panel first');
            this.app.showPropBrowser(true);
            return null;
          }
          const prop = PlacesOps.addProp(this.app.level, {
            model,
            x: snapped.x,
            z: snapped.z,
            rotation_degrees: Number(this.app.toolOptions.propRotation) || 0
          }, this.app.propCatalog);
          return prop ? prop.id : null;
        }, 'Add prop');
      case 'spawn':
        return this.placeAndDrag(() => {
          PlacesOps.setSpawn(this.app.level, {
            x: snapped.x,
            z: snapped.z,
            yaw_degrees: Number(this.app.toolOptions.spawnFacing) || this.app.level.spawn.yaw_degrees
          });
          return 'spawn';
        }, 'Move spawn');
      default:
        return;
    }
  }

  /** Light/prop/spawn: place on press, then let the same drag fine-tune the position. */
  placeAndDrag(create, label) {
    const id = create();
    if (!id) return;
    this.dragMode = 'place';
    this.placedId = id;
    this.dragActionLabel = label;
    this.dragInitial.clear();
    const found = PlacesOps.findObject(this.app.level, id);
    if (found) this.dragInitial.set(id, { x: found.object.x, z: found.object.z });
    this.select(id);
    this.app.levelChanged();
    this.app.requestRender();
  }

  beginSelectDrag(pt, snapped, additive) {
    if (this.selectedIds.size === 1) {
      const id = [...this.selectedIds][0];
      const handle = this.hitTestHandle(id, pt);
      if (handle) {
        this.dragMode = 'resize';
        this.activeHandle = handle;
        this.resizeTarget = id;
        this.resizeInitial = this.captureResizeInitial(id);
        this.app.requestRender();
        return;
      }
    }

    const hit = PlacesOps.hitTest2D(this.app.level, this.snapPoint(this.renderer.screenToWorld(pt.x, pt.y)), this.hitTolerance(), this.app.propCatalog);
    if (hit) {
      if (additive) this.select(hit.id, true);
      else if (!this.selectedIds.has(hit.id)) this.select(hit.id);

      this.dragMode = 'move';
      this.dragWorldStart = snapped;
      this.dragInitial.clear();
      for (const id of this.selectedIds) this.captureMoveInitial(id);
      this.app.requestRender();
      return;
    }

    if (!additive) this.clearSelection();
    this.dragMode = 'marquee';
    this.marqueeBox = { x1: pt.x, y1: pt.y, x2: pt.x, y2: pt.y };
    this.app.requestRender();
  }

  captureMoveInitial(id) {
    const opening = PlacesOps.findOpening(this.app.level, id);
    if (opening) {
      this.dragInitial.set(id, { offset: opening.opening.offset });
      return;
    }
    const found = PlacesOps.findObject(this.app.level, id);
    if (found) this.dragInitial.set(id, { x: found.object.x, z: found.object.z });
  }

  captureResizeInitial(id) {
    const opening = PlacesOps.findOpening(this.app.level, id);
    if (opening) {
      return { offset: opening.opening.offset, width: opening.opening.width, sill: opening.opening.sill, height: opening.opening.height };
    }
    const bounds = PlacesOps.objectBounds2D(this.app.level, id, this.app.propCatalog);
    return bounds ? { x: bounds.x, z: bounds.z, width: bounds.width, depth: bounds.depth } : null;
  }

  /** Size preset for a new opening, honouring the tool option bar. */
  placementSize(kind) {
    const preset = PlacesOps.defaultOpening(kind);
    const options = this.app.toolOptions || {};
    if (kind === 'window') {
      return {
        width: Number(options.windowWidth) || preset.width,
        height: Number(options.windowHeight) || preset.height,
        sill: options.windowSill === undefined ? preset.sill : Number(options.windowSill)
      };
    }
    if (kind === 'door') {
      return {
        width: Number(options.doorWidth) || preset.width,
        height: Number(options.doorHeight) || preset.height,
        sill: 0
      };
    }
    return preset;
  }

  /** Door/window: press on a wall, drag along it to set position and width. */
  beginOpeningDrag(snapped) {
    const wall = PlacesOps.wallAtPoint(this.app.level, snapped, this.hitTolerance(0.35));
    if (!wall) {
      this.app.updateStatus(`Click on a wall to place a ${this.currentTool}`);
      return;
    }
    // The anchor matches the hover ghost (centred on the click), so a click places
    // the opening exactly where the preview showed it.
    const preset = this.placementSize(this.currentTool);
    const clicked = this.snap(PlacesGeometry.wallProjectOffset(wall, snapped));
    const anchor = Math.max(0, Math.min(PlacesGeometry.wallLength(wall) - preset.width, clicked - preset.width / 2));
    this.activeOpening = { wall, kind: this.currentTool, startOffset: anchor };
    this.dragMode = 'opening';
    this.dragInitial.clear();
    this.updateOpeningPreview(anchor);
    this.app.requestRender();
  }

  /** Builds the ghost opening: default size on a click, dragged span on a drag. */
  updateOpeningPreview(currentOffset) {
    if (!this.activeOpening) return;
    const { wall, kind, startOffset } = this.activeOpening;
    const preset = this.placementSize(kind);
    const length = PlacesGeometry.wallLength(wall);
    const start = Math.max(0, Math.min(startOffset, currentOffset));
    const end = Math.min(length, Math.max(startOffset, currentOffset));
    const width = Math.min(length, Math.max(preset.width, end - start));
    const offset = Math.max(0, Math.min(length - width, start));
    this.preview = {
      type: 'opening',
      kind,
      width,
      offset,
      rect: PlacesOps.openingBounds2D(wall, { offset, width })
    };
  }

  onMouseMove(e) {
    const pt = this.getCanvasPoint(e);
    if (pt.x < -1000 || pt.y < -1000) return; // synthetic events
    const world = this.renderer.screenToWorld(pt.x, pt.y);
    const snapped = this.snapPoint(world);
    this.app.updateCoords(snapped.x, snapped.z);

    if (Math.abs(pt.x - this.dragStart.x) > 3 || Math.abs(pt.y - this.dragStart.y) > 3) this.wasDragged = true;

    if (this.isPanning) {
      const dx = (e.clientX - this.panStart.x) / this.renderer.zoom;
      const dz = (e.clientY - this.panStart.y) / this.renderer.zoom;
      this.renderer.cameraX = this.cameraStart.x - dx;
      this.renderer.cameraZ = this.cameraStart.z - dz;
      this.app.requestRender();
      return;
    }

    // Hover feedback for the select tool.
    if (this.currentTool === 'select' && !this.dragMode) {
      if (this.selectedIds.size === 1) {
        const handle = this.hitTestHandle([...this.selectedIds][0], pt);
        if (handle) {
          this.updateCursor(handle);
          return;
        }
      }
      this.updateCursor();
    }

    // Hover feedback for the door/window tools: show the opening before clicking.
    if ((this.currentTool === 'door' || this.currentTool === 'window') && !this.dragMode) {
      const wall = PlacesOps.wallAtPoint(this.app.level, snapped, this.hitTolerance(0.35));
      if (!wall) {
        if (this.preview) {
          this.preview = null;
          this.app.requestRender();
        }
      } else {
        const preset = PlacesOps.defaultOpening(this.currentTool);
        const offset = this.snap(PlacesGeometry.wallProjectOffset(wall, snapped) - preset.width / 2);
        const width = Math.min(preset.width, PlacesGeometry.wallLength(wall));
        const clamped = Math.max(0, Math.min(PlacesGeometry.wallLength(wall) - width, offset));
        this.preview = {
          type: 'opening',
          kind: this.currentTool,
          width,
          rect: PlacesOps.openingBounds2D(wall, { offset: clamped, width })
        };
        this.app.requestRender();
      }
    }

    if (this.dragMode === 'move') {
      const dx = snapped.x - this.dragWorldStart.x;
      const dz = snapped.z - this.dragWorldStart.z;
      const level = this.app.level;
      for (const [id, initial] of this.dragInitial) {
        if (initial.offset !== undefined) {
          const opening = PlacesOps.findOpening(level, id);
          if (opening) {
            const length = PlacesGeometry.wallLength(opening.wall);
            opening.opening.offset = Number(Math.max(0, Math.min(length - opening.opening.width, initial.offset + dx)).toFixed(4));
          }
        } else {
          PlacesOps.moveObjectTo(level, id, Number((initial.x + dx).toFixed(4)), Number((initial.z + dz).toFixed(4)));
        }
      }
      this.app.levelChanged();
      return;
    }

    if (this.dragMode === 'resize' && this.resizeTarget) {
      PlacesOps.resizeObject(this.app.level, this.resizeTarget, this.activeHandle, snapped, this.resizeInitial);
      this.app.levelChanged();
      return;
    }

    if (this.dragMode === 'marquee' && this.marqueeBox) {
      this.marqueeBox.x2 = pt.x;
      this.marqueeBox.y2 = pt.y;
      this.app.requestRender();
      return;
    }

    if (this.dragMode === 'draw' && this.preview && this.preview.type === 'rect') {
      this.preview.current = snapped;
      this.app.requestRender();
      return;
    }

    if (this.dragMode === 'opening' && this.activeOpening) {
      const offset = this.snap(PlacesGeometry.wallProjectOffset(this.activeOpening.wall, snapped));
      this.updateOpeningPreview(offset);
      this.app.requestRender();
      return;
    }

    if (this.dragMode === 'place' && this.placedId) {
      PlacesOps.moveObjectTo(this.app.level, this.placedId, snapped.x, snapped.z);
      this.app.levelChanged();
      return;
    }
  }

  onMouseUp(e) {
    if (this.isPanning) {
      this.isPanning = false;
      this.updateCursor();
      return;
    }

    const pt = this.getCanvasPoint(e);
    const snapped = this.snapPoint(this.renderer.screenToWorld(pt.x, pt.y));

    if (this.dragMode === 'move') {
      if (this.wasDragged) this.app.commit('Move');
      this.dragMode = null;
      this.dragInitial.clear();
      return;
    }

    if (this.dragMode === 'resize') {
      if (this.wasDragged) this.app.commit('Resize');
      this.dragMode = null;
      this.activeHandle = null;
      this.resizeTarget = null;
      this.resizeInitial = null;
      return;
    }

    if (this.dragMode === 'place') {
      // The object was created on mouse-down, so this is always a real action.
      this.app.commit(this.dragActionLabel || 'Place');
      this.dragMode = null;
      this.placedId = null;
      return;
    }

    if (this.dragMode === 'marquee') {
      if (this.marqueeBox && (Math.abs(this.marqueeBox.x2 - this.marqueeBox.x1) > 3 || Math.abs(this.marqueeBox.y2 - this.marqueeBox.y1) > 3)) {
        const w1 = this.renderer.screenToWorld(Math.min(this.marqueeBox.x1, this.marqueeBox.x2), Math.min(this.marqueeBox.y1, this.marqueeBox.y2));
        const w2 = this.renderer.screenToWorld(Math.max(this.marqueeBox.x1, this.marqueeBox.x2), Math.max(this.marqueeBox.y1, this.marqueeBox.y2));
        const ids = PlacesOps.objectsInRect(this.app.level, {
          x: Math.min(w1.x, w2.x), z: Math.min(w1.z, w2.z),
          width: Math.abs(w2.x - w1.x), depth: Math.abs(w2.z - w1.z)
        }, this.app.propCatalog);
        this.selectMany(ids, e.shiftKey);
        if (ids.length > 0) this.app.updateStatus(`Selected ${ids.length} object${ids.length === 1 ? '' : 's'}`);
      }
      this.dragMode = null;
      this.marqueeBox = null;
      this.app.requestRender();
      return;
    }

    if (this.dragMode === 'draw') {
      this.finishDraw(snapped, pt);
      return;
    }

    if (this.dragMode === 'opening') {
      this.finishOpening();
      return;
    }
  }

  finishDraw(snapped, pt) {
    const preview = this.preview;
    this.dragMode = null;
    this.preview = null;
    if (!preview) return;

    const start = preview.start;
    const moved = Math.abs(snapped.x - start.x) > 0.2 || Math.abs(snapped.z - start.z) > 0.2;
    const isClick = !this.wasDragged || !moved;

    if (this.currentTool === 'room') {
      const rect = isClick
        ? { x: snapped.x - 3, z: snapped.z - 3, width: 6, depth: 6 }
        : { x: Math.min(start.x, snapped.x), z: Math.min(start.z, snapped.z), width: Math.abs(snapped.x - start.x), depth: Math.abs(snapped.z - start.z) };
      if (rect.width < 0.5 || rect.depth < 0.5) return;
      const room = PlacesOps.createRoom(this.app.level, { ...rect, height: this.app.level.getCeilingHeight() });
      if (this.app.toolOptions.roomWalls) this.addWallsAround(rect);
      this.select(room.id);
      this.app.commit('Add room');
      this.app.updateStatus(`Room ${rect.width.toFixed(1)} × ${rect.depth.toFixed(1)} m added`);
      return;
    }

    if (this.currentTool === 'wall') {
      const thickness = Math.max(0.05, this.app.toolOptions.wallThickness || 0.35);
      const height = this.app.toolOptions.wallHeight; // null = full height
      let wall = null;
      if (isClick) {
        wall = PlacesOps.createWall(this.app.level, {
          x: snapped.x - 1, z: snapped.z - thickness / 2, width: 2, depth: thickness, height
        });
      } else {
        const dx = Math.abs(snapped.x - start.x);
        const dz = Math.abs(snapped.z - start.z);
        if (dx >= dz) {
          wall = PlacesOps.createWall(this.app.level, {
            x: Math.min(start.x, snapped.x), z: Math.min(start.z, snapped.z) - thickness / 2 + (snapped.z - start.z) / 2,
            width: dx, depth: thickness, height
          });
        } else {
          wall = PlacesOps.createWall(this.app.level, {
            x: Math.min(start.x, snapped.x) - thickness / 2 + (snapped.x - start.x) / 2, z: Math.min(start.z, snapped.z),
            width: thickness, depth: dz, height
          });
        }
      }
      if (!wall) return;
      this.select(wall.id);
      this.app.commit('Add wall');
      this.app.updateStatus(`Wall ${PlacesGeometry.wallLength(wall).toFixed(2)} m added`);
      return;
    }

    if (this.currentTool === 'patch') {
      if (isClick) return;
      const rect = {
        x: Math.min(start.x, snapped.x), z: Math.min(start.z, snapped.z),
        width: Math.abs(snapped.x - start.x), depth: Math.abs(snapped.z - start.z)
      };
      if (rect.width < 0.25 || rect.depth < 0.25) return;
      const patch = PlacesOps.addFloorPatch(this.app.level, { ...rect, material: this.app.toolOptions.patchMaterial });
      this.select(patch.id);
      this.app.commit('Add floor patch');
      return;
    }
    void pt;
  }

  /** Creates the four perimeter walls of a room rectangle, skipping duplicates. */
  addWallsAround(rect) {
    PlacesOps.addWallsAroundRect(this.app.level, rect, {
      thickness: this.app.toolOptions.wallThickness || 0.35,
      height: this.app.level.getCeilingHeight()
    });
  }

  finishOpening() {
    const active = this.activeOpening;
    const preview = this.preview;
    this.dragMode = null;
    this.activeOpening = null;
    this.preview = null;
    if (!active) return;

    const preset = this.placementSize(active.kind);
    const opening = PlacesOps.addOpening(active.wall, {
      kind: active.kind,
      offset: preview ? preview.offset : this.snap(active.startOffset - preset.width / 2),
      width: preview ? preview.width : preset.width,
      height: preset.height,
      sill: preset.sill
    });
    if (!opening) return;
    this.select(opening.id);
    this.app.commit(active.kind === 'door' ? 'Add doorway' : 'Add window');
    this.app.updateStatus(`${active.kind === 'door' ? 'Doorway' : 'Window'} ${opening.width.toFixed(2)} m placed — adjust it in the inspector`);
  }

  hitTolerance(fallback = 0.2) {
    return Math.max(fallback, this.renderer.screenDistToWorld(6));
  }

  /** Resize handles for the current selection (rectangle objects or opening jambs). */
  hitTestHandle(id, pt) {
    const level = this.app.level;
    const opening = PlacesOps.findOpening(level, id);
    const tolerance = 7;

    if (opening) {
      const { wall, opening: value } = opening;
      const axis = PlacesGeometry.wallAxis(wall);
      const thickness = PlacesGeometry.wallThickness(wall);
      const mid = thickness / 2;
      const a = PlacesGeometry.wallLocalToWorld(wall, value.offset, mid);
      const b = PlacesGeometry.wallLocalToWorld(wall, value.offset + value.width, mid);
      const sa = this.renderer.worldToScreen(a.x, a.z);
      const sb = this.renderer.worldToScreen(b.x, b.z);
      const candidates = axis === 'x'
        ? [['w', sa], ['e', sb]]
        : [['n', sa], ['s', sb]];
      for (const [name, point] of candidates) {
        if (Math.abs(pt.x - point.x) <= tolerance && Math.abs(pt.y - point.y) <= tolerance) return name;
      }
      return null;
    }

    const bounds = PlacesOps.objectBounds2D(level, id, this.app.propCatalog);
    if (!bounds) return null;
    const s = this.renderer.worldToScreen(bounds.x, bounds.z);
    const w = this.renderer.worldDistToScreen(bounds.width);
    const h = this.renderer.worldDistToScreen(bounds.depth);
    const handles = {
      nw: { x: s.x, y: s.y },
      n: { x: s.x + w / 2, y: s.y },
      ne: { x: s.x + w, y: s.y },
      e: { x: s.x + w, y: s.y + h / 2 },
      se: { x: s.x + w, y: s.y + h },
      s: { x: s.x + w / 2, y: s.y + h },
      sw: { x: s.x, y: s.y + h },
      w: { x: s.x, y: s.y + h / 2 }
    };
    for (const [name, point] of Object.entries(handles)) {
      if (Math.abs(pt.x - point.x) <= tolerance && Math.abs(pt.y - point.y) <= tolerance) return name;
    }
    return null;
  }

  cancelOperation() {
    this.dragMode = null;
    this.preview = null;
    this.marqueeBox = null;
    this.activeOpening = null;
    this.dragInitial.clear();
    this.resizeTarget = null;
    this.resizeInitial = null;
    this.activeHandle = null;
    this.app.requestRender();
  }
}
