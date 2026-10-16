#!/usr/bin/env python3
"""Launch Places through one stable macOS validation app; never build or bake."""
from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
BUNDLE_ID = "io.github.csd113.places.validation"
APP_NAME = "Places Validation.app"
EXECUTABLE = "places-validation"


def digest(path: Path) -> str:
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(block)
    return checksum.hexdigest()


def prepare_app(binary: Path, directory: Path) -> dict:
    """Reuse bundle bytes for identical builds; refuse another app's bundle."""
    app = directory / APP_NAME
    contents = app / "Contents"
    plist = contents / "Info.plist"
    executable = contents / "MacOS" / EXECUTABLE
    for path in (app, contents, plist, executable.parent, executable):
        if path.is_symlink():
            raise ValueError(f"validation bundle must not contain symlinks: {path}")
    if app.exists():
        if not plist.is_file() or plistlib.loads(plist.read_bytes()).get("CFBundleIdentifier") != BUNDLE_ID:
            raise ValueError(f"refusing to replace an unrelated app: {app}")
    executable.parent.mkdir(parents=True, exist_ok=True)
    data = plistlib.dumps({
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleName": "Places Validation",
        "CFBundleDisplayName": "Places Validation",
        "CFBundleExecutable": EXECUTABLE,
        "CFBundlePackageType": "APPL",
        "CFBundleVersion": "1",
        "NSHighResolutionCapable": True,
    })
    if not plist.exists() or plist.read_bytes() != data:
        plist.write_bytes(data)
    binary_hash = digest(binary)
    if not executable.exists() or digest(executable) != binary_hash:
        with tempfile.NamedTemporaryFile(dir=executable.parent, delete=False) as temporary:
            staged = Path(temporary.name)
        try:
            shutil.copy2(binary, staged)
            staged.chmod(staged.stat().st_mode | 0o111)
            staged.replace(executable)
        finally:
            staged.unlink(missing_ok=True)
    return {
        "bundle_path": str(app), "bundle_identifier": BUNDLE_ID,
        "executable": str(executable), "binary_sha256": binary_hash,
        "plist_sha256": digest(plist),
    }


def launch_command(identity: dict, out: Path, environment: dict) -> list[str]:
    command = ["/usr/bin/open", "-W", "-n", "--stdout", str(out / "stdout.log"),
               "--stderr", str(out / "stderr.log")]
    for key, value in sorted(environment.items()):
        if not isinstance(key, str) or not key.startswith("PLACES_") or "=" in key or not isinstance(value, str):
            raise ValueError("run environment must contain PLACES_* string values only")
        command.extend(("--env", f"{key}={value}"))
    command.append(identity["bundle_path"])
    return command


@contextmanager
def exclusive_run(directory: Path):
    import fcntl  # Only the macOS launch path needs the native advisory lock.

    directory.mkdir(parents=True, exist_ok=True)
    with (directory / "session.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError("another Places validation launch is still active") from error
        yield


def ensure_stopped(directory: Path) -> None:
    executable = str(directory / APP_NAME / "Contents" / "MacOS" / EXECUTABLE)
    rows = subprocess.check_output(["ps", "-axo", "pid=,comm="], text=True).splitlines()
    if any(len(parts := row.strip().split(None, 1)) == 2 and parts[1] == executable for row in rows):
        raise ValueError("Places Validation is still running; close it before preparing another run")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release/places")
    parser.add_argument("--out", type=Path, required=True, help="new run directory, separate from the app")
    parser.add_argument("--env-json", type=Path, help="object containing PLACES_* string values")
    parser.add_argument("--settings-json", type=Path, help="initial settings for this isolated run")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("this foreground validation launcher requires macOS")
    binary = args.binary.resolve(strict=True)
    if binary.name != "places" or not binary.is_relative_to(ROOT):
        parser.error("use a Places executable named places from this checkout or its preserved local builds")
    out = args.out.resolve()
    directory = ROOT / "local/native-validation"
    if out == directory or directory in out.parents:
        parser.error("run output must be outside local/native-validation")
    environment = json.loads(args.env_json.read_text()) if args.env_json else {}
    if not isinstance(environment, dict):
        parser.error("--env-json must contain an object")
    settings = json.loads(args.settings_json.read_text()) if args.settings_json else None
    if settings is not None and not isinstance(settings, dict):
        parser.error("--settings-json must contain an object")
    environment.setdefault("PLACES_ASSET_ROOT", str(ROOT))
    environment["PLACES_STATE_ROOT"] = str(out)
    with exclusive_run(directory):
        ensure_stopped(directory)
        # Validate configuration before touching the bundle or creating results.
        launch_command({"bundle_path": "unused"}, out, environment)
        out.mkdir(parents=True, exist_ok=False)
        if settings is not None:
            (out / "settings.json").write_text(json.dumps(settings, indent=2) + "\n")
        identity = prepare_app(binary, directory)
        record = {**identity, "source_binary": str(binary), "environment": environment}
        (out / "identity.json").write_text(json.dumps(record, indent=2) + "\n")
        print(json.dumps(identity), flush=True)
        clean_env = {key: value for key, value in os.environ.items() if not key.startswith("PLACES_")}
        started = time.monotonic()
        # No automatic retry/timeout: an approval delay must not spawn a duplicate.
        child = subprocess.Popen(launch_command(identity, out, environment), env=clean_env)
        rss_samples = []
        while child.poll() is None:
            rows = subprocess.check_output(["ps", "-axo", "rss=,comm="], text=True).splitlines()
            for row in rows:
                parts = row.strip().split(None, 1)
                if len(parts) == 2 and parts[1] == identity["executable"]:
                    rss_samples.append({"seconds": time.monotonic() - started,
                                        "rss_bytes": int(parts[0]) * 1024})
            try:
                child.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass
        (out / "launch.json").write_text(json.dumps({
            "returncode": child.returncode, "seconds": time.monotonic() - started,
            "native_rss_samples": rss_samples,
            "peak_native_rss_bytes": max((row["rss_bytes"] for row in rss_samples), default=None),
        }, indent=2) + "\n")
        return child.returncode


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, plistlib.InvalidFileException) as error:
        print(f"Places validation: {error}", file=sys.stderr)
        raise SystemExit(1) from error
