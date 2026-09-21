// io.js - JSON and ZIP level package import/export for Liminal Level Editor

class LevelIO {
  constructor(app) {
    this.app = app;
  }

  // Trigger browser download for a Blob
  downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    setTimeout(() => {
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    }, 100);
  }

  // Convert base64 dataUrl to Uint8Array
  dataUrlToUint8Array(dataUrl) {
    const parts = dataUrl.split(',');
    const binary = atob(parts[1]);
    const len = binary.length;
    const bytes = new Uint8Array(len);
    for (let i = 0; i < len; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  // Convert Uint8Array to PNG DataURL
  bytesToPngDataUrl(bytes) {
    let binary = '';
    const len = bytes.byteLength;
    for (let i = 0; i < len; i++) {
      binary += String.fromCharCode(bytes[i]);
    }
    return 'data:image/png;base64,' + btoa(binary);
  }

  // EXPORT JSON
  exportJSON(level) {
    const validation = validateLevel(level);
    if (!validation.valid) {
      const msg = "Level has validation errors:\n" + validation.errors.join("\n") + "\n\nExport anyway?";
      if (!confirm(msg)) return;
    }

    const jsonString = JSON.stringify(level.toJSON(), null, 2);
    const blob = new Blob([jsonString], { type: 'application/json' });
    const filename = `${level.id || 'level'}.json`;
    this.downloadBlob(blob, filename);
    this.app.updateStatus(`Exported JSON level: ${filename}`);
  }

  // EXPORT ZIP LEVEL PACK
  async exportZIP(level) {
    if (typeof JSZip === 'undefined') {
      alert("JSZip library not loaded. Cannot create ZIP archive.");
      return;
    }

    const validation = validateLevel(level);
    if (!validation.valid) {
      const msg = "Level has validation errors:\n" + validation.errors.join("\n") + "\n\nExport anyway?";
      if (!confirm(msg)) return;
    }

    this.app.updateStatus("Generating ZIP level pack...");
    const zip = new JSZip();

    // 1. level.json
    const levelJson = JSON.stringify(level.toJSON(), null, 2);
    zip.file("level.json", levelJson);

    // 2. materials.json and textures/
    const customTextures = level.custom_textures || {};
    const customEntries = Object.entries(customTextures);

    if (customEntries.length > 0) {
      const materialsObj = { materials: {} };
      const texturesFolder = zip.folder("textures");

      for (const [matId, tex] of customEntries) {
        // Ensure standard pack: namespace and filename
        const filename = tex.filename.replace(/^textures\//, '');
        materialsObj.materials[matId] = {
          texture: `textures/${filename}`
        };

        // Add binary png to textures/
        if (tex.bytes) {
          texturesFolder.file(filename, tex.bytes);
        } else if (tex.dataUrl) {
          const bytes = this.dataUrlToUint8Array(tex.dataUrl);
          texturesFolder.file(filename, bytes);
        }
      }

      zip.file("materials.json", JSON.stringify(materialsObj, null, 2));
    }

    try {
      const blob = await zip.generateAsync({ type: 'blob' });
      const filename = `${level.id || 'level'}.zip`;
      this.downloadBlob(blob, filename);
      this.app.updateStatus(`Exported ZIP level pack: ${filename}`);
    } catch (err) {
      console.error("ZIP generation error:", err);
      alert("Failed to generate ZIP: " + err.message);
      this.app.updateStatus("Failed to export ZIP");
    }
  }

  // IMPORT FILE (auto-detects JSON vs ZIP)
  async importFile(file) {
    const filename = file.name.toLowerCase();
    this.app.updateStatus(`Importing ${file.name}...`);

    if (filename.endsWith('.json')) {
      const text = await file.text();
      return this.importJSONString(text, file.name);
    } else if (filename.endsWith('.zip')) {
      return this.importZIPFile(file);
    } else {
      alert("Unsupported file format. Please open a .json level or a .zip level pack.");
      this.app.updateStatus("Import failed: unsupported file type");
    }
  }

  // IMPORT JSON STRING
  importJSONString(jsonStr, sourceName = 'level.json') {
    try {
      const data = JSON.parse(jsonStr);

      if (data.format_version && data.format_version !== 1) {
        alert(`Warning: Level format_version is ${data.format_version}, but liminal-rust requires 1.`);
      }

      const newLevel = new Level(data);
      this.app.setLevel(newLevel, `Loaded ${sourceName}`);
      this.app.renderer.fitToGeometry(newLevel);
      this.app.updateStatus(`Successfully imported ${sourceName} (${newLevel.walls.length} walls, ${newLevel.ceiling_lights.length} lights)`);
      return true;
    } catch (err) {
      console.error("Failed to parse level JSON:", err);
      alert(`Invalid level JSON in ${sourceName}:\n${err.message}`);
      this.app.updateStatus(`Failed to parse ${sourceName}`);
      return false;
    }
  }

  // IMPORT ZIP FILE
  async importZIPFile(file) {
    if (typeof JSZip === 'undefined') {
      alert("JSZip library not loaded. Cannot extract ZIP archive.");
      return false;
    }

    try {
      const zip = await JSZip.loadAsync(file);

      // Locate level.json
      let levelJsonFile = null;
      let materialsJsonFile = null;
      const textureFiles = [];

      zip.forEach((relativePath, zipEntry) => {
        if (zipEntry.dir) return;
        const norm = relativePath.toLowerCase().replace(/\\/g, '/');
        const filename = norm.split('/').pop();

        if (filename === 'level.json') {
          levelJsonFile = zipEntry;
        } else if (filename === 'materials.json') {
          materialsJsonFile = zipEntry;
        } else if (norm.includes('textures/') || norm.endsWith('.png')) {
          textureFiles.push({ relativePath, zipEntry, filename });
        }
      });

      if (!levelJsonFile) {
        alert("ZIP archive does not contain a valid 'level.json' file.");
        this.app.updateStatus("Corrupt level pack: missing level.json");
        return false;
      }

      const levelJsonStr = await levelJsonFile.async('text');
      const levelData = JSON.parse(levelJsonStr);
      const newLevel = new Level(levelData);

      // Parse materials.json if present
      let materialMap = {};
      if (materialsJsonFile) {
        const matJsonStr = await materialsJsonFile.async('text');
        try {
          const matObj = JSON.parse(matJsonStr);
          const materialsList = matObj.materials || matObj;
          for (const [k, v] of Object.entries(materialsList)) {
            if (typeof v === 'string') {
              materialMap[k] = v;
            } else if (v && typeof v === 'object' && v.texture) {
              materialMap[k] = v.texture;
            }
          }
        } catch (e) {
          console.warn("Error parsing materials.json in ZIP:", e);
        }
      }

      // Extract custom textures
      for (const tf of textureFiles) {
        const bytes = await tf.zipEntry.async('uint8array');
        const dataUrl = this.bytesToPngDataUrl(bytes);

        // Measure dimensions
        const dims = await this.getImageDimensions(dataUrl);

        // Find or derive material ID
        let matId = null;
        for (const [k, targetPath] of Object.entries(materialMap)) {
          const normTarget = targetPath.toLowerCase().replace(/\\/g, '/');
          const targetName = normTarget.split('/').pop();
          if (normTarget === tf.relativePath.toLowerCase() || targetName === tf.filename) {
            matId = k;
            break;
          }
        }

        if (!matId) {
          const stem = tf.filename.replace(/\.[^/.]+$/, "");
          matId = `pack:${stem}`;
        }

        newLevel.custom_textures[matId] = {
          filename: `textures/${tf.filename}`,
          dataUrl,
          width: dims.width,
          height: dims.height,
          bytes
        };
      }

      this.app.setLevel(newLevel, `Loaded pack ${file.name}`);
      this.app.renderer.fitToGeometry(newLevel);
      this.app.updateStatus(`Successfully imported level pack ${file.name} with ${Object.keys(newLevel.custom_textures).length} custom texture(s)`);
      return true;
    } catch (err) {
      console.error("Failed to extract ZIP level pack:", err);
      alert(`Error extracting ZIP level pack:\n${err.message}`);
      this.app.updateStatus("Failed to extract ZIP");
      return false;
    }
  }

  getImageDimensions(dataUrl) {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        resolve({ width: img.naturalWidth || 64, height: img.naturalHeight || 64 });
      };
      img.onerror = () => {
        resolve({ width: 64, height: 64 });
      };
      img.src = dataUrl;
    });
  }

  // Setup drag and drop on window
  setupDragAndDrop() {
    window.addEventListener('dragover', (e) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    });

    window.addEventListener('drop', (e) => {
      e.preventDefault();
      if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
        this.importFile(e.dataTransfer.files[0]);
      }
    });
  }
}
