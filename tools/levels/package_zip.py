"""ZIP views for explicit developer tools, using bounded Rust for method 93.

Legacy reads and all writes retain stdlib behavior. A Zstandard payload read
creates one temporary exact-record Deflate copy through package-repack, then
reuses it until close. Accounting still observes original names, sizes and
compression methods. This is tooling compatibility, not the game's loader or
a performance measurement of the temporary re-encoding. Close entry streams
before their archive. No Python Zstandard decoder is used, even on Python 3.14.
"""
from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]


class PackageZip(zipfile.ZipFile):
    """Preserve original ZIP metadata and bound all method-93 decompression."""

    def __init__(self, *args, **kwargs):
        self._decoded = None
        self._scratch = None
        super().__init__(*args, **kwargs)

    def _bounded_view(self):
        if self._decoded is not None:
            return self._decoded
        if not self.filename or not Path(self.filename).is_file():
            raise zipfile.BadZipFile("bounded Zstandard tooling requires a saved package path")
        executable = "package-repack.exe" if os.name == "nt" else "package-repack"
        reader = Path(os.environ.get("PLACES_PACKAGE_REPACK", ROOT / "target/release" / executable))
        if not reader.is_file():
            raise zipfile.BadZipFile("build package-repack or set PLACES_PACKAGE_REPACK for bounded Zstandard reads")
        scratch = tempfile.TemporaryDirectory(prefix="places-package-read-")
        decoded = None
        try:
            output = Path(scratch.name) / "bounded-deflate.zip"
            result = subprocess.run([str(reader), str(Path(self.filename).resolve()), str(output), "1"],
                                    capture_output=True, text=True, check=False)
            if result.returncode:
                raise zipfile.BadZipFile("bounded Rust package read failed: " + result.stderr.strip())
            decoded = zipfile.ZipFile(output)
            expected = [(i.filename, i.file_size, i.CRC) for i in self.infolist()]
            actual = [(i.filename, i.file_size, i.CRC) for i in decoded.infolist()]
            if (sorted(expected) != sorted(actual)
                    or any(i.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                           for i in decoded.infolist())):
                raise zipfile.BadZipFile("bounded compatibility copy changed record identities")
        except BaseException:
            if decoded is not None:
                decoded.close()
            scratch.cleanup()
            raise
        self._scratch = scratch
        self._decoded = decoded
        return decoded

    def open(self, name, mode="r", pwd=None, *, force_zip64=False):
        if mode == "r":
            info = name if isinstance(name, zipfile.ZipInfo) else self.getinfo(name)
            if info.compress_type == 93:
                return self._bounded_view().open(info.filename, mode, pwd=pwd)
        return super().open(name, mode, pwd=pwd, force_zip64=force_zip64)

    def close(self):
        decoded, scratch = self._decoded, self._scratch
        self._decoded = None
        self._scratch = None
        try:
            super().close()
        finally:
            if decoded is not None:
                decoded.close()
            if scratch is not None:
                scratch.cleanup()
