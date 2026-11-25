"""ZIP views for explicit developer tools, using bounded Rust for method 93.

Legacy reads and all writes retain stdlib behavior. A Zstandard payload read
creates one temporary exact-record Deflate copy through the map compiler, then
reuses it until close. Accounting still observes original names, sizes and
compression methods. This is tooling compatibility, not the game's loader or
a performance measurement of the temporary re-encoding. Close entry streams
before their archive. No Python Zstandard decoder is used, even on Python 3.14.
"""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BLOCK_BYTES = 256 * 1024


def hash_stream(stream):
    """Return the byte count and SHA-256 of the bytes read to EOF."""
    digest = hashlib.sha256()
    size = 0
    while chunk := stream.read(BLOCK_BYTES):
        size += len(chunk)
        digest.update(chunk)
    return size, digest.hexdigest()


def archive_identity(stat):
    return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns


def archive_path_identity(path):
    # Use the same metadata API as the original open handle. On Windows a
    # pathname stat and a handle stat can report different change times
    # for the same unchanged file.
    with path.open("rb", buffering=0) as handle:
        return archive_identity(os.fstat(handle.fileno()))


class PackageZip(zipfile.ZipFile):
    """Preserve original ZIP metadata and bound all method-93 decompression."""

    def __init__(self, file, mode="r", *args, **kwargs):
        self._decoded = None
        self._scratch = None
        self._source = None
        self._identity = None
        if mode == "r" and isinstance(file, (str, os.PathLike)):
            # Capture identity on the handle before ZipFile reads its directory.
            self._source = Path(file).open("rb", buffering=0)
            file = self._source
        try:
            if self._source is not None:
                self._identity = archive_identity(os.fstat(self._source.fileno()))
            super().__init__(file, mode, *args, **kwargs)
            self.verify_source_identity()
        except BaseException:
            self.close()
            raise

    def verify_source_identity(self):
        """Recheck the identity of the open source handle for path-based reads."""
        if self._source is not None and archive_identity(os.fstat(self._source.fileno())) != self._identity:
            raise zipfile.BadZipFile("package changed since opening the archive")

    @contextmanager
    def raw_stream(self):
        """Read the original ZIP bytes from the same handle as the ZIP metadata."""
        with self._lock:
            self.verify_source_identity()
            position = self.fp.tell()
            self.fp.seek(0)
            try:
                yield self.fp
                self.verify_source_identity()
            finally:
                self.fp.seek(position)

    def _bounded_view(self):
        self.verify_source_identity()
        if self._decoded is not None:
            return self._decoded
        if not self.filename or not Path(self.filename).is_file():
            raise zipfile.BadZipFile("bounded Zstandard tooling requires a saved package path")
        executable = "places-map-compiler.exe" if os.name == "nt" else "places-map-compiler"
        reader = Path(os.environ.get("PLACES_COMPILER", ROOT / "target/release" / executable))
        if not reader.is_file():
            raise zipfile.BadZipFile("build places-map-compiler or set PLACES_COMPILER for bounded Zstandard reads")
        scratch = tempfile.TemporaryDirectory(prefix="places-package-read-")
        decoded = None
        try:
            output = Path(scratch.name) / "bounded-deflate.zip"
            source = Path(self.filename).resolve()
            if self._source is not None and archive_path_identity(source) != self._identity:
                raise zipfile.BadZipFile("package path changed since opening the archive")
            result = subprocess.run([str(reader), "--debug", "repack", str(source), str(output), "1"],
                                    capture_output=True, text=True, check=False)
            self.verify_source_identity()
            if self._source is not None and archive_path_identity(source) != self._identity:
                raise zipfile.BadZipFile("package path changed since opening the archive")
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
            if getattr(self, "fp", None) is not None:
                super().close()
        finally:
            if self._source is not None:
                self._source.close()
                self._source = None
            if decoded is not None:
                decoded.close()
            if scratch is not None:
                scratch.cleanup()
