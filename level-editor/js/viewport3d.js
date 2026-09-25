// viewport3d.js - WebGL 1 realtime 3D preview for the level editor.
// Draws the shared level mesh from js/geometry.js with the same single static
// shader, vertex-coloured triangle batches and repeat-wrapped 64x64 procedural
// tiles as the game's own renderer (see src/render.rs), and adds a fly camera,
// X-ray walls, an optional floor grid and a line-box selection overlay.
//
// The editor app owns the level: this module only reads `app.level`, reports
// picks through app.viewportSelect and reports world-space XZ drag deltas
// through app.viewportDragBegin/Update/End. It never mutates the level itself.
// Camera maths lives in js/camera3d.js; every method here is cheap to call from
// an animation frame and no-ops while the canvas has zero size.

(function (root) {
  'use strict';

  const DEG = Math.PI / 180;
  const HIGHLIGHT_COLOR = [0.25, 0.95, 1.0, 1.0];
  const GRID_COLOR = [0.35, 0.45, 0.5, 0.4];
  const XRAY_ALPHA = 0.35;
  const DEFAULT_LEVEL_BOUNDS = { min: [-10, 0, -10], max: [10, 4, 10] };
  const NO_SELECTION = new Set();
  const MOVEMENT_KEYS = new Set(['w', 'a', 's', 'd', 'q', 'e', ' ', 'shift', 'control']);
  const EDGE_BITS = [1, 2, 4];

  const VERTEX_SRC = [
    'attribute vec3 a_pos;',
    'attribute vec4 a_color;',
    'attribute vec2 a_uv;',
    'uniform mat4 u_mvp;',
    'varying vec4 v_color;',
    'varying vec2 v_uv;',
    'void main() {',
    '  v_color = a_color;',
    '  v_uv = a_uv;',
    '  gl_Position = u_mvp * vec4(a_pos, 1.0);',
    '}'
  ].join('\n');

  const FRAGMENT_SRC = [
    'precision mediump float;',
    'uniform sampler2D u_texture;',
    'uniform float u_alpha;',
    'varying vec4 v_color;',
    'varying vec2 v_uv;',
    'void main() {',
    '  vec4 texel = texture2D(u_texture, v_uv) * v_color;',
    '  gl_FragColor = vec4(texel.rgb, texel.a * u_alpha);',
    '}'
  ].join('\n');

  // --------------------------------------------------------------- shaders

  function compileShader(gl, type, source) {
    const shader = gl.createShader(type);
    if (!shader) return null;
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      if (typeof console !== 'undefined' && console.warn) {
        console.warn('Viewport3D: shader compile failed:', gl.getShaderInfoLog(shader));
      }
      gl.deleteShader(shader);
      return null;
    }
    return shader;
  }

  function createProgram(gl, vertexSource, fragmentSource) {
    const vertex = compileShader(gl, gl.VERTEX_SHADER, vertexSource);
    const fragment = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
    if (!vertex || !fragment) {
      if (vertex) gl.deleteShader(vertex);
      if (fragment) gl.deleteShader(fragment);
      return null;
    }
    const program = gl.createProgram();
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    // The program owns the compiled shaders after a successful link.
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      if (typeof console !== 'undefined' && console.warn) {
        console.warn('Viewport3D: shader link failed:', gl.getProgramInfoLog(program));
      }
      gl.deleteProgram(program);
      return null;
    }
    return program;
  }

  // -------------------------------------------------------------- textures

  function clampByte(value) {
    return value < 0 ? 0 : value > 255 ? 255 : value | 0;
  }

  /** Deterministic per-texel hash in [0, 1); the integer mix of src/render.rs. */
  function hash01(x, y, seed) {
    let h = Math.imul(x | 0, 0x9e3779b9) ^ Math.imul(y | 0, 0x85ebca6b) ^ Math.imul(seed | 0, 0xc2b2ae35);
    h ^= h >>> 15;
    h = Math.imul(h, 0x2545f491);
    h ^= h >>> 13;
    h = Math.imul(h, 0x27d4eb2d);
    h ^= h >>> 16;
    return (h >>> 8) / 16777216;
  }

  /** Tileable value noise: the wrapped-lattice sampler of src/render.rs. */
  function tileNoise(x, y, size, period, seed) {
    const p = Math.max(1, period | 0);
    const scale = p / size;
    const fx = x * scale;
    const fy = y * scale;
    const x0 = Math.floor(fx);
    const y0 = Math.floor(fy);
    const tx = fx - x0;
    const ty = fy - y0;
    const sx = tx * tx * (3 - 2 * tx);
    const sy = ty * ty * (3 - 2 * ty);
    const wrap = (v) => ((v % p) + p) % p;
    const v00 = hash01(wrap(x0), wrap(y0), seed);
    const v10 = hash01(wrap(x0 + 1), wrap(y0), seed);
    const v01 = hash01(wrap(x0), wrap(y0 + 1), seed);
    const v11 = hash01(wrap(x0 + 1), wrap(y0 + 1), seed);
    const top = v00 + (v10 - v00) * sx;
    const bottom = v01 + (v11 - v01) * sx;
    return top + (bottom - top) * sy;
  }

  function tileNoise2(x, y, size, coarse, fine, seed) {
    const value = tileNoise(x, y, size, coarse, seed) * 0.65
      + tileNoise(x, y, size, fine, seed + 7) * 0.35;
    return value < 0 ? 0 : value > 1 ? 1 : value;
  }

  /**
   * Builds one of the game's procedural surface tiles as ImageData, mirroring
   * generate_wall_texture / generate_carpet_texture / generate_ceiling_texture
   * in src/render.rs so the preview matches in-game materials. Wallpaper and
   * ceiling cover two metres per repeat, the carpet one.
   */
  function paintTile(kind) {
    const doc = root && root.document;
    if (!doc || typeof doc.createElement !== 'function') return null;
    const size = kind === 'floor' ? 64 : 128;
    const canvas = doc.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext && canvas.getContext('2d');
    if (!ctx || typeof ctx.createImageData !== 'function') return null;
    const image = ctx.createImageData(size, size);
    const data = image.data;
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        const i = (y * size + x) * 4;
        let r = 0;
        let g = 0;
        let b = 0;
        if (kind === 'wall') {
          const phase = x % 16;
          let tone = phase < 7 ? 0.997 : 0.928;
          if (phase === 7 || phase === 15) tone *= 0.96;
          else if (phase === 3) tone *= 1.014;
          const fibre = (hash01(x, y, 11) - 0.5) * 0.03;
          const age = tileNoise2(x, y, 128, 6, 17, 23) - 0.5;
          tone *= 1 + fibre + 0.075 * age;
          r = 243 * tone;
          g = 237 * tone * (1 - 0.008 * age);
          b = 220 * tone * (1 - 0.022 * age);
        } else if (kind === 'floor') {
          const speckle = hash01(x, y, 31) - 0.5;
          const dashV = hash01(x, y >> 1, 37) - 0.5;
          const dashH = hash01(x >> 1, y, 41) - 0.5;
          const tuft = tileNoise(x, y, 64, 21, 45) - 0.5;
          const mottle = tileNoise(x, y, 64, 5, 43) - 0.5;
          const broad = tileNoise(x, y, 64, 13, 47) - 0.5;
          const warm = tileNoise(x, y, 64, 3, 53) - 0.5;
          const tone = 1 + 0.07 * speckle + 0.05 * dashV + 0.035 * dashH
            + 0.035 * tuft + 0.06 * mottle + 0.04 * broad;
          r = 231 * tone * (1 + 0.02 * warm);
          g = 223 * tone;
          b = 210 * tone * (1 - 0.028 * warm);
        } else {
          const tx = x % 64;
          const ty = y % 64;
          const edge = Math.min(tx, 63 - tx, ty, 63 - ty);
          const tile = Math.floor(x / 64) + 2 * Math.floor(y / 64);
          const tileTone = 1 + (hash01(tile, tile * 7, 61) - 0.5) * 0.024;
          const fibre = (hash01(x, y, 67) - 0.5) * 0.045;
          const pores = hash01(x, y, 71) > 0.945 ? -0.075 : 0;
          const blotch = tileNoise(x, y, 128, 9, 73) - 0.5;
          const field = tileTone * (1 + fibre + pores + 0.035 * blotch);
          const dipTable = [0.52, 0.66, 0.84, 0.95];
          const dip = edge < 4 ? dipTable[edge] : 1;
          const barMix = edge < 2 ? 1 - edge * 0.35 : 0;
          const tileColor = [247, 247, 242];
          const barColor = [168, 168, 162];
          r = tileColor[0] * field * dip * (1 - barMix) + barColor[0] * barMix * field;
          g = tileColor[1] * field * dip * (1 - barMix) + barColor[1] * barMix * field;
          b = tileColor[2] * field * dip * (1 - barMix) + barColor[2] * barMix * field;
        }
        data[i] = clampByte(r);
        data[i + 1] = clampByte(g);
        data[i + 2] = clampByte(b);
        data[i + 3] = 255;
      }
    }
    return image;
  }

  // ------------------------------------------------------------- utilities

  function geometryAPI() {
    if (root && root.PlacesGeometry) return root.PlacesGeometry;
    if (typeof PlacesGeometry !== 'undefined') return PlacesGeometry;
    return null;
  }

  function cameraAPI() {
    if (root && root.PlacesCamera3D) return root.PlacesCamera3D;
    if (typeof PlacesCamera3D !== 'undefined') return PlacesCamera3D;
    return null;
  }

  function isTypingTarget(element) {
    if (!element || !element.tagName) return false;
    const tag = String(element.tagName).toUpperCase();
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || element.isContentEditable === true;
  }

  function normalizeKey(key) {
    if (key === ' ' || key === 'Spacebar') return ' ';
    return String(key == null ? '' : key).toLowerCase();
  }

  function clamp(value, min, max) {
    return value < min ? min : value > max ? max : value;
  }

  /** Appends the 12 edges of one oriented pick box as LINES positions. */
  function pushBoxEdges(out, box) {
    const center = (box && box.center) || [0, 0, 0];
    const half = (box && box.half) || [0.5, 0.5, 0.5];
    const rotation = ((Number(box && box.rotationY) || 0) * Math.PI) / 180;
    const cos = Math.cos(rotation);
    const sin = Math.sin(rotation);
    const corners = [];
    for (let i = 0; i < 8; i++) {
      const lx = (i & 1 ? 1 : -1) * half[0];
      const ly = (i & 2 ? 1 : -1) * half[1];
      const lz = (i & 4 ? 1 : -1) * half[2];
      // Same XZ transform as geometry.pushProp/add_prop_box, so the outline
      // hugs the drawn prop instead of mirroring it.
      corners.push([
        center[0] + lx * cos + lz * sin,
        center[1] + ly,
        center[2] - lx * sin + lz * cos
      ]);
    }
    for (let i = 0; i < 8; i++) {
      for (let b = 0; b < EDGE_BITS.length; b++) {
        const bit = EDGE_BITS[b];
        if (i & bit) continue;
        const a = corners[i];
        const c = corners[i | bit];
        out.push(a[0], a[1], a[2], c[0], c[1], c[2]);
      }
    }
  }

  // --------------------------------------------------------------- viewport

  class Viewport3D {
    /**
     * `canvas` is the 3D canvas element. `app` is the editor app:
     * { level, propCatalog, propProxies, editor.selectedIds, levelRevision,
     *   selectionRevision, viewportSelect, viewportDragBegin/Update/End,
     *   updateStatus, requestRender }.
     * Never throws: on any failure `isSupported()` returns false and the app
     * keeps running in 2D-only mode.
     */
    constructor(canvas, app) {
      this.canvas = canvas;
      this.app = app || {};
      this.gl = null;
      this.supported = false;
      this.disposed = false;

      // GL objects
      this._program = null;
      this._textures = null;
      this._posBuffer = null;
      this._colorBuffer = null;
      this._uvBuffer = null;
      this._highlightBuffer = null;
      this._gridBuffer = null;
      this._gridCount = 0;
      this._aPos = -1;
      this._aColor = -1;
      this._aUv = -1;
      this._uMvp = null;
      this._uTexture = null;
      this._uAlpha = null;

      // Mesh / overlay state, rebuilt only when a revision changes.
      this._batches = null;
      this._highlightCount = 0;
      this._meshRevision = -1;
      this._highlightRevision = -1;

      // View options
      this._xray = false;
      this._ceilingsMode = 'auto'; // 'auto' | 'on' | 'off'
      this._autoHideCeilings = true;
      this._showGrid = false;
      this._helpVisible = true;
      this._helpEl = null;

      // Input state
      this._keys = new Set();
      this._hovered = false;
      this._drag = null;
      this._lastFrameTime = null;
      this._listeners = [];
      this._lastPointer = { x: 0, y: 0 };
      this._ray = { origin: [0, 0, 0], direction: [0, 0, 0] };
      this._dragPoint = [0, 0, 0];
      this._rayPlaneY = null;

      // Reused every frame so the draw path does not allocate.
      this._vp = new Float32Array(16);

      try {
        const camera = cameraAPI();
        const geometry = geometryAPI();
        if (!camera || !camera.Camera3D) throw new Error('camera3d.js is not loaded');
        if (!geometry) throw new Error('geometry.js is not loaded');
        this._rayPlaneY = typeof camera.rayPlaneY === 'function' ? camera.rayPlaneY : null;
        this.camera = new camera.Camera3D({ yaw: 45 * DEG, pitch: -22 * DEG, fov: 65, far: 500 });
        this._initGL();
        this.supported = true;
      } catch (err) {
        this.camera = null;
        this.gl = null;
        this.supported = false;
        // Diagnostics only: the editor keeps working in 2D when 3D is unavailable.
        if (typeof console !== 'undefined' && console.warn) {
          console.warn('Places 3D preview unavailable:', err && err.message ? err.message : err);
        }
      }

      if (this.supported) {
        // Never call into the app here: the app has not stored this instance
        // yet, so it must not be asked for a frame before the constructor
        // returns. Startup pose is set directly instead.
        try { this.camera.reset(this._levelBounds()); } catch (err) { /* keep the default pose */ }
        try { this._createHelpOverlay(); } catch (err) { this._helpEl = null; }
        try { this._bindEvents(); } catch (err) { /* render-only mode */ }
      }
    }

    // ------------------------------------------------------------ GL setup

    _initGL() {
      const canvas = this.canvas;
      if (!canvas || typeof canvas.getContext !== 'function') throw new Error('no canvas element');
      const attributes = {
        alpha: false,
        antialias: true,
        depth: true,
        stencil: false,
        preserveDrawingBuffer: false,
        powerPreference: 'high-performance'
      };
      const gl = canvas.getContext('webgl', attributes) || canvas.getContext('experimental-webgl', attributes);
      if (!gl) throw new Error('WebGL is unavailable');
      this.gl = gl;

      this._program = createProgram(gl, VERTEX_SRC, FRAGMENT_SRC);
      if (!this._program) throw new Error('the preview shader failed to build');
      this._uMvp = gl.getUniformLocation(this._program, 'u_mvp');
      this._uTexture = gl.getUniformLocation(this._program, 'u_texture');
      this._uAlpha = gl.getUniformLocation(this._program, 'u_alpha');
      this._aPos = gl.getAttribLocation(this._program, 'a_pos');
      this._aColor = gl.getAttribLocation(this._program, 'a_color');
      this._aUv = gl.getAttribLocation(this._program, 'a_uv');

      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.disable(gl.CULL_FACE);
      gl.clearColor(0.05, 0.055, 0.07, 1);

      this._posBuffer = gl.createBuffer();
      this._colorBuffer = gl.createBuffer();
      this._uvBuffer = gl.createBuffer();
      this._highlightBuffer = gl.createBuffer();
      this._gridBuffer = gl.createBuffer();
      this._textures = {
        white: this._uploadTexture(new Uint8Array(16).fill(255), 2, 2),
        wall: this._uploadTexture(paintTile('wall'), 128, 128),
        floor: this._uploadTexture(paintTile('floor'), 64, 64),
        ceiling: this._uploadTexture(paintTile('ceiling'), 128, 128)
      };
      this._uploadGrid();

      if (typeof canvas.hasAttribute === 'function' && !canvas.hasAttribute('tabindex')) {
        canvas.tabIndex = 0; // let the canvas receive keyboard focus
      }
    }

    _uploadTexture(source, width, height) {
      const gl = this.gl;
      const texture = gl.createTexture();
      gl.bindTexture(gl.TEXTURE_2D, texture);
      if (source) {
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, source);
      } else {
        const blank = new Uint8Array(Math.max(1, width * height * 4));
        blank.fill(255);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, blank);
      }
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      return texture;
    }

    /** One static 1 m grid; the level mesh itself is the only dynamic geometry. */
    _uploadGrid() {
      const gl = this.gl;
      const half = 100;
      const vertices = [];
      for (let i = -half; i <= half; i++) {
        vertices.push(-half, 0.01, i, half, 0.01, i);
        vertices.push(i, 0.01, -half, i, 0.01, half);
      }
      this._gridCount = vertices.length / 3;
      gl.bindBuffer(gl.ARRAY_BUFFER, this._gridBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.STATIC_DRAW);
    }

    // ------------------------------------------------------------ contract

    /** Matches the canvas backing store to CSS size x devicePixelRatio. */
    resize() {
      if (!this.supported || this.disposed) return false;
      return this._syncSize();
    }

    _syncSize() {
      const canvas = this.canvas;
      const cssWidth = canvas.clientWidth;
      const cssHeight = canvas.clientHeight;
      if (!(cssWidth > 0) || !(cssHeight > 0)) return false; // hidden pane costs nothing
      const dpr = Math.max(root.devicePixelRatio || 1, 1);
      const width = Math.max(1, Math.round(cssWidth * dpr));
      const height = Math.max(1, Math.round(cssHeight * dpr));
      if (canvas.width !== width) canvas.width = width;
      if (canvas.height !== height) canvas.height = height;
      return true;
    }

    /** Draws one frame; cheap and safe to call from every animation frame. */
    render() {
      if (!this.supported || this.disposed || !this.gl) return;
      const gl = this.gl;
      if (!this._syncSize() || !this.camera) return;

      const app = this.app;
      const level = app ? app.level : null;
      const meshRevision = app && Number.isFinite(app.levelRevision) ? app.levelRevision : 0;
      const selectionRevision = app && Number.isFinite(app.selectionRevision) ? app.selectionRevision : 0;

      // Rebuilds are coalesced: at most one mesh and one overlay rebuild per
      // frame, however many revision bumps happened since the last one.
      let meshRebuilt = false;
      if (meshRevision !== this._meshRevision) {
        this._rebuildMesh(level);
        this._meshRevision = meshRevision;
        meshRebuilt = true;
      }
      if (meshRebuilt || selectionRevision !== this._highlightRevision) {
        this._rebuildHighlight(level);
        this._highlightRevision = selectionRevision;
      }

      const width = gl.drawingBufferWidth;
      const height = gl.drawingBufferHeight;
      gl.viewport(0, 0, width, height);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      this._updateMovement();
      if (!this._batches && !this._highlightCount && !this._showGrid) return;

      gl.useProgram(this._program);
      gl.uniformMatrix4fv(this._uMvp, false, this.camera.viewProjection(width / Math.max(1, height), this._vp));
      gl.uniform1i(this._uTexture, 0);
      gl.uniform1f(this._uAlpha, 1);
      gl.activeTexture(gl.TEXTURE0);

      // Opaque passes, matching the game's batch order; depth writing makes the
      // exact order irrelevant but keeps the X-ray pass correct.
      this._drawBatch('floor', this._textures.floor);
      this._drawBatch('props', this._textures.white);
      this._drawBatch('lights', this._textures.white);
      if (this._ceilingsVisible()) this._drawBatch('ceiling', this._textures.ceiling);
      if (!this._xray) this._drawBatch('walls', this._textures.wall);

      if (this._xray) {
        // Translucent walls after the opaque passes: blend, no depth writes.
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.depthMask(false);
        gl.uniform1f(this._uAlpha, XRAY_ALPHA);
        this._drawBatch('walls', this._textures.wall);
        gl.uniform1f(this._uAlpha, 1);
        gl.depthMask(true);
        gl.disable(gl.BLEND);
      }

      if (this._showGrid) this._drawGrid();
      this._drawHighlight();
    }

    /** Forces a mesh rebuild (and the derived highlight) on the next frame. */
    markDirty() {
      this._meshRevision = -1;
      this._highlightRevision = -1;
      this._requestRender();
    }

    /** Forces a selection-highlight rebuild on the next frame. */
    markSelectionDirty() {
      this._highlightRevision = -1;
      this._requestRender();
    }

    /** Frames the current selection, or the whole level when nothing is selected. */
    focusSelected() {
      if (!this.camera) return;
      const level = this.app ? this.app.level : null;
      const bounds = this._selectionBounds(level, this._selectionIds());
      if (bounds) this.camera.focusBounds(bounds, 1.25);
      else this.camera.focusBounds(this._levelBounds(), 1.15);
      this._requestRender();
    }

    /** Frames the whole level. */
    frameAll() {
      if (!this.camera) return;
      this.camera.focusBounds(this._levelBounds(), 1.15);
      this._requestRender();
    }

    /** Returns to the default three-quarter view of the level. */
    resetCamera() {
      if (!this.camera) return;
      this.camera.reset(this._levelBounds());
      this._requestRender();
    }

    /** See-through wall mode. */
    setXray(enabled) {
      this._xray = !!enabled;
      this._requestRender();
    }

    isXray() {
      return this._xray;
    }

    /** Pins the ceiling batch on or off; otherwise it auto-hides (see below). */
    setShowCeilings(enabled) {
      this._ceilingsMode = enabled ? 'on' : 'off';
      this._requestRender();
    }

    /**
     * 'auto' hides ceilings while the camera is above the lowest room ceiling (so
     * rooms stay readable), 'on' always draws them, 'off' never does.
     */
    setCeilingsMode(mode) {
      this._ceilingsMode = mode === 'on' || mode === 'off' ? mode : 'auto';
      this._requestRender();
    }

    ceilingsMode() {
      return this._ceilingsMode;
    }

    /**
     * Auto-hide ceilings when the eye rises above the level's lowest ceiling,
     * so flying above a level always looks into its rooms.
     */
    setAutoHideCeilings(enabled) {
      this._autoHideCeilings = !!enabled;
      this._requestRender();
    }

    /** Optional HUD floor grid. */
    setShowGrid(enabled) {
      this._showGrid = !!enabled;
      this._requestRender();
    }

    /** Shows or hides the camera-control hint overlay. */
    setHelpVisible(visible) {
      this._helpVisible = !!visible;
      if (this._helpEl) this._helpEl.style.display = this._helpVisible ? '' : 'none';
    }

    isSupported() {
      return !!this.supported && !this.disposed;
    }

    /** Releases GL resources, listeners and the help overlay. */
    dispose() {
      if (this.disposed) return;
      this.disposed = true;
      this._removeEvents();
      this._keys.clear();
      this._drag = null;
      const gl = this.gl;
      if (gl) {
        try {
          const buffers = [this._posBuffer, this._colorBuffer, this._uvBuffer, this._highlightBuffer, this._gridBuffer];
          for (let i = 0; i < buffers.length; i++) {
            if (buffers[i]) gl.deleteBuffer(buffers[i]);
          }
          if (this._textures) {
            for (const key in this._textures) {
              if (this._textures[key]) gl.deleteTexture(this._textures[key]);
            }
          }
          if (this._program) gl.deleteProgram(this._program);
        } catch (err) {
          // A lost context may already have released its objects.
        }
      }
      this.gl = null;
      this._program = null;
      this._textures = null;
      this._batches = null;
      this._highlightCount = 0;
      if (this._helpEl && this._helpEl.parentNode) this._helpEl.parentNode.removeChild(this._helpEl);
      this._helpEl = null;
      this.camera = null;
      this.supported = false;
    }

    // ------------------------------------------------------------ mesh build

    _rebuildMesh(level) {
      const gl = this.gl;
      this._batches = null;
      if (!level) return;
      const geometry = geometryAPI();
      if (!geometry || typeof geometry.buildLevelMesh !== 'function') return;
      let mesh = null;
      try {
        // Ceilings are always built; hiding them just skips the batch at draw
        // time so toggling visibility never rebuilds the mesh.
        mesh = geometry.buildLevelMesh(level, {
          catalog: this.app.propCatalog,
          proxies: this.app.propProxies,
          includeCeilings: true
        });
      } catch (err) {
        if (typeof console !== 'undefined' && console.warn) {
          console.warn('Viewport3D: level mesh build failed:', err);
        }
        return;
      }
      if (!mesh || !mesh.vertexCount) return;

      const batches = {};
      for (let i = 0; i < mesh.batches.length; i++) {
        const batch = mesh.batches[i];
        batches[batch.name] = { start: batch.start, count: batch.count, material: batch.material };
      }
      this._batches = batches;

      gl.bindBuffer(gl.ARRAY_BUFFER, this._posBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, mesh.positions, gl.STATIC_DRAW);
      gl.bindBuffer(gl.ARRAY_BUFFER, this._colorBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, mesh.colors, gl.STATIC_DRAW);
      gl.bindBuffer(gl.ARRAY_BUFFER, this._uvBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, mesh.uvs, gl.STATIC_DRAW);
    }

    _rebuildHighlight(level) {
      const gl = this.gl;
      this._highlightCount = 0;
      const geometry = geometryAPI();
      const selection = this._selectionIds();
      if (!level || !geometry || typeof geometry.pickBoxesFor !== 'function' || !selection.size) return;

      const vertices = [];
      for (const id of selection) {
        let boxes = null;
        try {
          boxes = geometry.pickBoxesFor(level, id, { catalog: this.app.propCatalog });
        } catch (err) {
          boxes = null;
        }
        if (!boxes) continue;
        for (let i = 0; i < boxes.length; i++) pushBoxEdges(vertices, boxes[i]);
      }
      if (!vertices.length) return;

      this._highlightCount = vertices.length / 3;
      gl.bindBuffer(gl.ARRAY_BUFFER, this._highlightBuffer);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(vertices), gl.DYNAMIC_DRAW);
    }

    // -------------------------------------------------------------- drawing

    _bindMeshAttributes() {
      const gl = this.gl;
      gl.bindBuffer(gl.ARRAY_BUFFER, this._posBuffer);
      gl.vertexAttribPointer(this._aPos, 3, gl.FLOAT, false, 0, 0);
      gl.enableVertexAttribArray(this._aPos);
      if (this._aColor >= 0) {
        gl.bindBuffer(gl.ARRAY_BUFFER, this._colorBuffer);
        gl.vertexAttribPointer(this._aColor, 4, gl.FLOAT, false, 0, 0);
        gl.enableVertexAttribArray(this._aColor);
      }
      if (this._aUv >= 0) {
        gl.bindBuffer(gl.ARRAY_BUFFER, this._uvBuffer);
        gl.vertexAttribPointer(this._aUv, 2, gl.FLOAT, false, 0, 0);
        gl.enableVertexAttribArray(this._aUv);
      }
    }

    _drawBatch(name, texture) {
      const batch = this._batches ? this._batches[name] : null;
      if (!batch || batch.count <= 0) return;
      this._bindMeshAttributes();
      const gl = this.gl;
      gl.bindTexture(gl.TEXTURE_2D, texture || this._textures.white);
      gl.drawArrays(gl.TRIANGLES, batch.start, batch.count);
    }

    _drawGrid() {
      const gl = this.gl;
      gl.bindBuffer(gl.ARRAY_BUFFER, this._gridBuffer);
      gl.vertexAttribPointer(this._aPos, 3, gl.FLOAT, false, 0, 0);
      gl.enableVertexAttribArray(this._aPos);
      if (this._aColor >= 0) {
        gl.disableVertexAttribArray(this._aColor);
        gl.vertexAttrib4f(this._aColor, GRID_COLOR[0], GRID_COLOR[1], GRID_COLOR[2], GRID_COLOR[3]);
      }
      if (this._aUv >= 0) {
        gl.disableVertexAttribArray(this._aUv);
        gl.vertexAttrib2f(this._aUv, 0, 0);
      }
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.depthMask(false);
      gl.bindTexture(gl.TEXTURE_2D, this._textures.white);
      gl.uniform1f(this._uAlpha, 0.6);
      gl.lineWidth(1);
      gl.drawArrays(gl.LINES, 0, this._gridCount);
      gl.uniform1f(this._uAlpha, 1);
      gl.depthMask(true);
      gl.disable(gl.BLEND);
    }

    _drawHighlight() {
      if (!this._highlightCount) return;
      const gl = this.gl;
      gl.bindBuffer(gl.ARRAY_BUFFER, this._highlightBuffer);
      gl.vertexAttribPointer(this._aPos, 3, gl.FLOAT, false, 0, 0);
      gl.enableVertexAttribArray(this._aPos);
      if (this._aColor >= 0) {
        gl.disableVertexAttribArray(this._aColor);
        gl.vertexAttrib4f(this._aColor, HIGHLIGHT_COLOR[0], HIGHLIGHT_COLOR[1], HIGHLIGHT_COLOR[2], HIGHLIGHT_COLOR[3]);
      }
      if (this._aUv >= 0) {
        gl.disableVertexAttribArray(this._aUv);
        gl.vertexAttrib2f(this._aUv, 0, 0);
      }
      gl.disable(gl.DEPTH_TEST); // selection stays visible through geometry
      gl.bindTexture(gl.TEXTURE_2D, this._textures.white);
      gl.lineWidth(2); // many drivers clamp to 1, which is fine
      gl.drawArrays(gl.LINES, 0, this._highlightCount);
      gl.enable(gl.DEPTH_TEST);
    }

    _ceilingsVisible() {
      if (this._ceilingsMode === 'on') return true;
      if (this._ceilingsMode === 'off') return false;
      if (!this._autoHideCeilings) return true;
      const level = this.app ? this.app.level : null;
      let minimum = 3.5;
      if (level && Array.isArray(level.rooms) && level.rooms.length) {
        let found = Infinity;
        for (let i = 0; i < level.rooms.length; i++) {
          const height = Number(level.rooms[i] && level.rooms[i].height);
          if (Number.isFinite(height)) found = Math.min(found, height);
        }
        if (Number.isFinite(found)) minimum = found;
      }
      return this.camera.position[1] <= minimum;
    }

    // ------------------------------------------------------------- framing

    _selectionIds() {
      const editor = this.app ? this.app.editor : null;
      const ids = editor ? editor.selectedIds : null;
      return ids && typeof ids.has === 'function' && typeof ids.size === 'number' ? ids : NO_SELECTION;
    }

    /** AABB of one object bounds entry, accounting for its Y rotation. */
    _expandBounds(target, bounds) {
      if (!bounds || !bounds.center || !bounds.half) return false;
      const rotation = ((Number(bounds.rotationY) || 0) * Math.PI) / 180;
      const cos = Math.abs(Math.cos(rotation));
      const sin = Math.abs(Math.sin(rotation));
      const hx = bounds.half[0] * cos + bounds.half[2] * sin;
      const hz = bounds.half[0] * sin + bounds.half[2] * cos;
      const hy = bounds.half[1];
      const cx = bounds.center[0];
      const cy = bounds.center[1];
      const cz = bounds.center[2];
      if (cx - hx < target.min[0]) target.min[0] = cx - hx;
      if (cy - hy < target.min[1]) target.min[1] = cy - hy;
      if (cz - hz < target.min[2]) target.min[2] = cz - hz;
      if (cx + hx > target.max[0]) target.max[0] = cx + hx;
      if (cy + hy > target.max[1]) target.max[1] = cy + hy;
      if (cz + hz > target.max[2]) target.max[2] = cz + hz;
      return true;
    }

    _newBoundsTarget() {
      return {
        min: [Infinity, Infinity, Infinity],
        max: [-Infinity, -Infinity, -Infinity]
      };
    }

    _finishBounds(target, pad) {
      if (!Number.isFinite(target.min[0])) return null;
      const p = Number.isFinite(pad) ? pad : 0;
      return {
        min: [target.min[0] - p, target.min[1] - p, target.min[2] - p],
        max: [target.max[0] + p, target.max[1] + p, target.max[2] + p]
      };
    }

    _levelBounds() {
      const level = this.app ? this.app.level : null;
      const geometry = geometryAPI();
      if (!level || !geometry || typeof geometry.objectBounds3D !== 'function') {
        return DEFAULT_LEVEL_BOUNDS;
      }
      const options = { catalog: this.app.propCatalog };
      const target = this._newBoundsTarget();
      let found = false;
      const add = (id) => {
        let bounds = null;
        try {
          bounds = geometry.objectBounds3D(level, id, options);
        } catch (err) {
          bounds = null;
        }
        if (bounds && this._expandBounds(target, bounds)) found = true;
      };
      const rooms = level.rooms || [];
      for (let i = 0; i < rooms.length; i++) add(rooms[i].id);
      const walls = level.walls || [];
      for (let i = 0; i < walls.length; i++) add(walls[i].id);
      const lights = level.ceiling_lights || [];
      for (let i = 0; i < lights.length; i++) add(lights[i].id);
      const props = level.props || [];
      for (let i = 0; i < props.length; i++) add(props[i].id);
      if (level.spawn) add('spawn');
      const bounds = this._finishBounds(target, 1);
      return found && bounds ? bounds : DEFAULT_LEVEL_BOUNDS;
    }

    _selectionBounds(level, ids) {
      const geometry = geometryAPI();
      if (!level || !geometry || typeof geometry.pickBoxesFor !== 'function' || !ids || !ids.size) return null;
      const options = { catalog: this.app.propCatalog };
      const target = this._newBoundsTarget();
      let found = false;
      for (const id of ids) {
        let boxes = null;
        try {
          boxes = geometry.pickBoxesFor(level, id, options);
        } catch (err) {
          boxes = null;
        }
        if (!boxes) continue;
        for (let i = 0; i < boxes.length; i++) {
          if (this._expandBounds(target, boxes[i])) found = true;
        }
      }
      if (!found) return null;
      return this._finishBounds(target, 0.25);
    }

    // -------------------------------------------------------------- picking

    /** Ray through a client-space pointer position, reusing the scratch ray. */
    _rayAt(clientX, clientY) {
      const canvas = this.canvas;
      if (!this.camera || !canvas || typeof canvas.getBoundingClientRect !== 'function') return null;
      const rect = canvas.getBoundingClientRect();
      const width = rect.width || canvas.clientWidth;
      const height = rect.height || canvas.clientHeight;
      if (!(width > 0) || !(height > 0)) return null;
      return this.camera.rayFromScreen(clientX - rect.left, clientY - rect.top, width, height, this._ray);
    }

    /** Nearest object under the pointer; rooms only when nothing solid was hit. */
    _pickAt(clientX, clientY) {
      const geometry = geometryAPI();
      const level = this.app ? this.app.level : null;
      if (!geometry || typeof geometry.pickObject !== 'function' || !level) return null;
      const ray = this._rayAt(clientX, clientY);
      if (!ray) return null;
      const catalog = this.app.propCatalog;
      let hit = null;
      try {
        hit = geometry.pickObject(level, ray, { catalog: catalog, includeRooms: false });
      } catch (err) {
        hit = null;
      }
      if (!hit) {
        try {
          hit = geometry.pickObject(level, ray, { catalog: catalog });
        } catch (err) {
          hit = null;
        }
      }
      return hit;
    }

    // ---------------------------------------------------------------- input

    _on(target, type, handler, options) {
      target.addEventListener(type, handler, options);
      this._listeners.push({ target: target, type: type, handler: handler, options: options });
    }

    _bindEvents() {
      const canvas = this.canvas;
      this._on(canvas, 'pointerdown', (e) => this._handlePointerDown(e));
      this._on(canvas, 'pointermove', (e) => this._handlePointerMove(e));
      this._on(canvas, 'pointerup', (e) => this._handlePointerUp(e));
      this._on(canvas, 'pointercancel', (e) => this._handlePointerUp(e));
      this._on(canvas, 'pointerenter', () => { this._hovered = true; });
      this._on(canvas, 'pointerleave', () => { this._hovered = false; });
      this._on(canvas, 'wheel', (e) => this._handleWheel(e), { passive: false });
      this._on(canvas, 'contextmenu', (e) => e.preventDefault());
      this._on(root, 'keydown', (e) => this._handleKeyDown(e));
      this._on(root, 'keyup', (e) => this._handleKeyUp(e));
      this._on(root, 'blur', () => this._clearInput());
      this._on(root, 'resize', () => {
        this.resize();
        this._requestRender();
      });
    }

    _removeEvents() {
      for (let i = 0; i < this._listeners.length; i++) {
        const entry = this._listeners[i];
        try {
          entry.target.removeEventListener(entry.type, entry.handler, entry.options);
        } catch (err) {
          // Ignore targets that already went away.
        }
      }
      this._listeners.length = 0;
    }

    _clearInput() {
      this._keys.clear();
      const drag = this._drag;
      this._drag = null;
      if (drag && drag.mode === 'move' && drag.began) this._call('viewportDragEnd');
    }

    _requestRender() {
      const fn = this.app ? this.app.requestRender : null;
      if (typeof fn === 'function') fn.call(this.app);
    }

    _call(name) {
      const fn = this.app ? this.app[name] : null;
      if (typeof fn !== 'function') return;
      if (arguments.length === 1) {
        fn.call(this.app);
        return;
      }
      fn.apply(this.app, Array.prototype.slice.call(arguments, 1));
    }

    _isInteractive() {
      if (this._hovered) return true;
      const doc = root && root.document;
      return !!doc && doc.activeElement === this.canvas;
    }

    /** Advances WASD/QE flight by the real frame delta; never during a drag. */
    _updateMovement() {
      const now = typeof performance !== 'undefined' && performance && performance.now
        ? performance.now()
        : Date.now();
      const previous = this._lastFrameTime;
      this._lastFrameTime = now;
      if (previous === null || this._drag) return;
      if (!this._keys.size || !this.camera) return;
      const dt = clamp((now - previous) / 1000, 0, 0.1);
      if (dt <= 0) return;
      this.camera.moveWithKeys(this._keys, dt);
    }

    _handleKeyDown(event) {
      if (!this.supported || this.disposed || !this.camera) return;
      if (isTypingTarget(event.target) || (root.document && isTypingTarget(root.document.activeElement))) return;
      if (!this._isInteractive()) return;
      const key = normalizeKey(event.key);
      if (key === 'escape') return; // the app cancels operations with Escape
      if (MOVEMENT_KEYS.has(key)) {
        this._keys.add(key);
        if (!event.ctrlKey && !event.metaKey && !event.altKey) event.preventDefault();
        this._requestRender(); // keep frames coming for on-demand renderers
        return;
      }
      if (key === 'f') {
        this.focusSelected();
        return;
      }
      if (key === 'r') {
        this.resetCamera();
      }
    }

    _handleKeyUp(event) {
      // Always release, even while typing, so keys cannot stick.
      if (this.disposed) return;
      this._keys.delete(normalizeKey(event.key));
    }

    _handleWheel(event) {
      if (!this.supported || this.disposed || !this.camera) return;
      const delta = event.deltaY !== 0 ? event.deltaY : event.deltaX;
      if (delta === 0) return;
      if (!event.ctrlKey && !event.metaKey) event.preventDefault();
      if (event.shiftKey) {
        // Shift+wheel adjusts the fly speed instead of dollying.
        const factor = delta < 0 ? 1.15 : 1 / 1.15;
        this.camera.moveSpeed = clamp(this.camera.moveSpeed * factor, 0.5, 50);
        this._requestRender();
        return;
      }
      const step = Math.max(0.3, this.camera.panDistance * 0.12);
      this.camera.dolly(delta < 0 ? step : -step);
      this._requestRender();
    }

    _dragModeForButton(event) {
      if (event.button === 2) return 'look';
      if (event.button === 1) return 'pan';
      if (event.button === 0) return this._keys.has(' ') ? 'pan' : 'select';
      return null;
    }

    _handlePointerDown(event) {
      if (!this.supported || this.disposed) return;
      // Defensive: a lost pointerup must not leave a half-finished drag behind.
      if (this._drag) {
        const stale = this._drag;
        this._drag = null;
        if (stale.mode === 'move' && stale.began) this._call('viewportDragEnd');
      }
      this._lastPointer.x = event.clientX;
      this._lastPointer.y = event.clientY;
      const doc = root && root.document;
      if (typeof this.canvas.focus === 'function' && (!doc || doc.activeElement !== this.canvas)) {
        try {
          this.canvas.focus({ preventScroll: true });
        } catch (err) {
          try { this.canvas.focus(); } catch (err2) { /* not focusable */ }
        }
      }
      const mode = this._dragModeForButton(event);
      if (!mode) return;
      event.preventDefault();
      if (mode === 'select') {
        this._beginSelectDrag(event);
        return;
      }
      this._drag = { mode: mode, pointerId: event.pointerId };
      this._capturePointer(event.pointerId);
    }

    _beginSelectDrag(event) {
      const hit = this._pickAt(event.clientX, event.clientY);
      const additive = !!(event.shiftKey || event.ctrlKey || event.metaKey);
      let drag = {
        mode: 'select',
        pointerId: event.pointerId,
        hit: hit,
        additive: additive,
        startX: event.clientX,
        startY: event.clientY,
        moved: false
      };
      // A left drag that starts on the current selection moves it on XZ; a
      // click on an unselected object only selects it.
      if (hit && !additive && this._selectionIds().has(hit.id) && hit.bounds) {
        const baseY = (hit.bounds.center[1] || 0) - (hit.bounds.half[1] || 0);
        const ray = this._rayAt(event.clientX, event.clientY);
        const point = ray && this._rayPlaneY && Math.abs(ray.direction[1]) > 0.05
          ? this._rayPlaneY(ray.origin, ray.direction, baseY, this._dragPoint)
          : null;
        if (point) {
          drag = {
            mode: 'move',
            pointerId: event.pointerId,
            baseY: baseY,
            additive: additive,
            startX: event.clientX,
            startY: event.clientY,
            moved: false,
            began: false,
            last: [point[0], point[1], point[2]]
          };
        }
      }
      this._drag = drag;
      this._capturePointer(event.pointerId);
    }

    _handlePointerMove(event) {
      const dx = event.clientX - this._lastPointer.x;
      const dy = event.clientY - this._lastPointer.y;
      this._lastPointer.x = event.clientX;
      this._lastPointer.y = event.clientY;
      const drag = this._drag;
      if (!drag || drag.pointerId !== event.pointerId || !this.camera) return;
      if (drag.mode === 'look') {
        this.camera.look(dx, dy);
        this._requestRender();
        return;
      }
      if (drag.mode === 'pan') {
        this.camera.pan(dx, dy, this.canvas.clientWidth, this.canvas.clientHeight);
        this._requestRender();
        return;
      }
      if (drag.mode === 'select') {
        if (Math.abs(event.clientX - drag.startX) + Math.abs(event.clientY - drag.startY) > 3) drag.moved = true;
        return;
      }
      if (drag.mode === 'move') this._dragUpdate(event);
    }

    _dragUpdate(event) {
      const drag = this._drag;
      const ray = this._rayAt(event.clientX, event.clientY);
      // Grazing rays make the XZ intersection jump, so ignore near-horizontal views.
      if (!ray || !this._rayPlaneY || Math.abs(ray.direction[1]) < 0.05) return;
      const point = this._rayPlaneY(ray.origin, ray.direction, drag.baseY, this._dragPoint);
      if (!point) return;
      const travelled = Math.abs(event.clientX - drag.startX) + Math.abs(event.clientY - drag.startY);
      if (!drag.began) {
        if (travelled <= 3) return;
        drag.began = true; // one history entry per drag, owned by the app
        this._call('viewportDragBegin');
        drag.last[0] = point[0];
        drag.last[1] = point[1];
        drag.last[2] = point[2];
        return;
      }
      const stepX = point[0] - drag.last[0];
      const stepZ = point[2] - drag.last[2];
      drag.last[0] = point[0];
      drag.last[1] = point[1];
      drag.last[2] = point[2];
      this._call('viewportDragUpdate', stepX, stepZ);
    }

    _handlePointerUp(event) {
      const drag = this._drag;
      if (drag && drag.pointerId === event.pointerId) {
        this._drag = null;
        if (drag.mode === 'move') {
          if (drag.began) {
            this._call('viewportDragEnd');
            this._requestRender();
          }
        } else if (drag.mode === 'select' && !drag.moved) {
          if (drag.hit) this._call('viewportSelect', drag.hit.id, drag.additive);
          else this._call('viewportSelect', null, false);
        }
      }
      this._releasePointer(event.pointerId);
    }

    _capturePointer(pointerId) {
      try {
        if (this.canvas.setPointerCapture) this.canvas.setPointerCapture(pointerId);
      } catch (err) {
        // Capture is best-effort; drags still work inside the canvas.
      }
    }

    _releasePointer(pointerId) {
      try {
        if (this.canvas.releasePointerCapture && this.canvas.hasPointerCapture &&
            this.canvas.hasPointerCapture(pointerId)) {
          this.canvas.releasePointerCapture(pointerId);
        }
      } catch (err) {
        // Ignore stale pointer ids.
      }
    }

    // ------------------------------------------------------------------- HUD

    _createHelpOverlay() {
      const doc = root && root.document;
      const parent = (this.canvas && this.canvas.parentElement) || (doc && doc.body);
      if (!doc || !parent || typeof doc.createElement !== 'function') return;
      const help = doc.createElement('div');
      if (!help || !help.style) return;
      help.className = 'viewport3d-help';
      help.textContent = 'Right-drag look · WASD move · Wheel dolly · F focus · R reset · ? help';
      help.style.position = 'absolute';
      help.style.right = '10px';
      help.style.bottom = '10px';
      help.style.padding = '4px 8px';
      help.style.font = '11px/1.4 system-ui, sans-serif';
      help.style.color = 'rgba(255, 255, 255, 0.85)';
      help.style.background = 'rgba(12, 14, 18, 0.72)';
      help.style.borderRadius = '4px';
      help.style.pointerEvents = 'none';
      help.style.whiteSpace = 'nowrap';
      help.style.zIndex = '5';
      help.style.display = this._helpVisible ? '' : 'none';
      parent.appendChild(help);
      this._helpEl = help;
    }
  }

  if (root) root.Viewport3D = Viewport3D;
})(typeof window !== 'undefined' ? window : null);
