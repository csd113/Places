#!/usr/bin/env python3
"""GitHub-hosted application packaging and additive uploads to an existing draft.

Never create tags/releases, publish a draft, replace assets, or bake gameplay maps.
The small application-owned menu is prepared with each platform's compiler.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import urllib.parse
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'target/hosted-release'
PLATFORMS = ('macOS-arm64', 'Windows')


def run(*args: str | Path, **kwargs) -> str:
    try:
        return subprocess.check_output(list(map(str, args)), text=True, encoding='utf8', **kwargs).strip()
    except subprocess.CalledProcessError as error:
        print(error.output, file=sys.stderr)
        raise


def digest(path: Path) -> str:
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2) + '\n', encoding='utf8')


def version() -> str:
    value = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf8'))['package']['version']
    if not re.fullmatch(r'\d+\.\d+\.\d+', value):
        raise ValueError('Expected a numeric Cargo release version')
    lock = tomllib.loads((ROOT / 'Cargo.lock').read_text(encoding='utf8'))
    packages = [p['version'] for p in lock['package'] if p['name'] == 'places']
    changelog = re.search(r'^## (\S+) — (?:Unreleased|\d{4}-\d{2}-\d{2})$',
                          (ROOT / 'CHANGELOG.md').read_text(encoding='utf8'), re.M)
    inventory = json.loads((ROOT / 'tools/release-projects.json').read_text(encoding='utf8'))
    if packages != [value] or not changelog or changelog[1] != value or inventory['version'] != value:
        raise ValueError('Cargo.lock, changelog and map inventory must match Cargo.toml')
    return value


def identity() -> dict:
    return dict(version=version(), source_commit=run('git', 'rev-parse', 'HEAD', cwd=ROOT),
                repository=os.environ['GITHUB_REPOSITORY'], run_id=os.environ['GITHUB_RUN_ID'])


def api(endpoint: str) -> object:
    return json.loads(run('gh', 'api', endpoint))


def verify_tag(info: dict) -> str:
    """Require the explicit tag to still resolve to the built checkout."""
    tag = f"v{info['version']}"
    if os.environ.get('GITHUB_REF') != f'refs/tags/{tag}':
        raise ValueError(f'Upload requires running this workflow on the existing {tag} tag')
    repo = info['repository']
    encoded = urllib.parse.quote(tag, safe='')
    commit = api(f'repos/{repo}/commits/{encoded}')
    if commit['sha'] != info['source_commit']:
        raise ValueError('Release tag moved or does not match the compiled source')
    return tag


def destination(info: dict) -> dict:
    """Discover drafts with the upload job's push access, including older pages."""
    tag = verify_tag(info)
    # The by-tag endpoint is documented for published releases. The list endpoint
    # includes drafts for callers with push access; build tokens stay read-only.
    matches = []
    page = 1
    while True:
        releases = api(f"repos/{info['repository']}/releases?per_page=100&page={page}")
        matches.extend(release for release in releases if release['tag_name'] == tag)
        if len(releases) < 100:
            break
        page += 1
    if len(matches) != 1:
        raise ValueError('Expected exactly one accessible release for the selected tag')
    release = matches[0]
    if not release['draft'] or release.get('immutable', False):
        raise ValueError('Target must be an existing mutable draft release; published releases are untouched')
    return release


def preflight() -> None:
    info = identity()
    if os.environ.get('UPLOAD_TO_DRAFT') == 'true':
        verify_tag(info)
    with open(os.environ['GITHUB_OUTPUT'], 'a', encoding='utf8') as stream:
        stream.write(f"version={info['version']}\n")
    print(json.dumps(info, indent=2))


def archive_payload(stage: Path, archive: Path) -> None:
    """Preserve POSIX executable modes and reject redirected payload entries."""
    with zipfile.ZipFile(archive, 'x', zipfile.ZIP_DEFLATED, compresslevel=6) as output:
        for path in sorted(stage.rglob('*')):
            if path.is_symlink():
                raise ValueError(f'Unexpected payload symlink: {path}')
            if path.is_file():
                output.write(path, path.relative_to(stage).as_posix())
    with zipfile.ZipFile(archive) as check:
        bad = check.testzip()
        if bad:
            raise ValueError(f'Archive CRC failure: {bad}')


def package() -> None:
    if os.environ.get('GITHUB_ACTIONS') != 'true':
        raise ValueError('Package stage is for disposable hosted checkouts; use the ordinary local packagers')
    platform = os.environ['RELEASE_PLATFORM']
    if platform not in PLATFORMS or (platform == 'Windows') != (os.name == 'nt'):
        raise ValueError('Wrong native packaging host')
    info = identity()
    uploads = OUTPUT / 'uploads'
    uploads.mkdir(parents=True, exist_ok=True)
    stage = OUTPUT / 'stage'
    suffix = '.exe' if os.name == 'nt' else ''
    compiler = ROOT / f'target/release/places-compile{suffix}'
    if os.name == 'nt':
        runtime = Path(os.environ['PLACES_WINDOWS_RUNTIME'])
        # Headless reflection capture requires DXC beside the compiler, not only
        # on PATH. Install the same runtime used by the package before any CLI.
        for path in runtime.glob('*.dll'):
            shutil.copy2(path, compiler.parent / path.name)
    run(sys.executable, ROOT / 'tools/verify-luau-release.py', '--compiler', compiler)
    menu = OUTPUT / 'menu.placesmap'
    # Preparation identity includes target/compiler inputs. Never reuse an old
    # platform's baked menu or bypass verify-menu.py's current-source check.
    run(compiler, 'export-project', ROOT / 'projects/menu', '--out', menu,
        '--variants', 'static,full', '--asset-root', ROOT / 'assets',
        '--workers', '3', '--progress', 'off')
    if os.name == 'nt':
        run(sys.executable, ROOT / 'tools/package-windows.py', stage, '--with-editor',
            '--runtime-dir', runtime, '--sdl-license', runtime / 'SDL3-LICENSE.txt',
            '--menu-package', menu)
    else:
        bundled = ROOT / 'assets/shared/menu/scene.bin'
        original = bundled.read_bytes()
        try:
            shutil.copyfile(menu, bundled)
            run('sh', ROOT / 'tools/package.sh', '--with-editor', stage)
        finally:
            bundled.write_bytes(original)
        run('codesign', '--verify', '--deep', '--strict', stage / 'Places.app')
    # Smoke command-line discovery away from the checkout with no asset override.
    environment = {k: v for k, v in os.environ.items() if not k.startswith('PLACES_')}
    with tempfile.TemporaryDirectory(prefix='places-release-smoke-') as scratch:
        environment['PLACES_STATE_ROOT'] = scratch
        for binary, flag in [('places', '--list-levels'), ('places-compile', '--help')]:
            run(stage / f'Places/{binary}{suffix}', flag, cwd=scratch, env=environment)
    archive = uploads / f"Places-{info['version']}-{platform}.zip"
    archive_payload(stage, archive)
    info.update(platform=platform, archive=archive.name, bytes=archive.stat().st_size,
                sha256=digest(archive), rust=run('rustc', '-Vv'),
                menu_sha256=digest(menu), gameplay_maps_included=False,
                signing='ad-hoc; not notarized' if os.name != 'nt' else 'unsigned',
                native_gui_tested=False,
                runner_image=os.environ.get('ImageVersion'),
                rustflags=os.environ.get('RUSTFLAGS', ''))
    write_json(uploads / f'{platform}.json', info)
    print(json.dumps(info, indent=2))


def verify_artifacts(folder: Path, info: dict) -> list[dict]:
    records = []
    expected = set()
    for platform in PLATFORMS:
        record = json.loads((folder / f'{platform}.json').read_text(encoding='utf8'))
        name = f"Places-{info['version']}-{platform}.zip"
        for key in ('version', 'source_commit', 'repository', 'run_id'):
            if record[key] != info[key]:
                raise ValueError(f'{platform}: artifact {key} differs from this run')
        if record['platform'] != platform or record['archive'] != name:
            raise ValueError('Unexpected artifact name/platform')
        path = folder / name
        if path.is_symlink() or digest(path) != record['sha256'] or path.stat().st_size != record['bytes']:
            raise ValueError(f'Artifact integrity failure: {name}')
        expected.update((name, f'{platform}.json'))
        records.append(record)
    if {p.name for p in folder.iterdir()} != expected:
        raise ValueError('Unexpected/missing files in downloaded artifact set')
    return records


def upload_plan(records: list[dict], assets: list[dict]) -> list[str]:
    """Validate every collision before uploading anything. Resume identical bytes only."""
    existing = {asset['name']: asset for asset in assets}
    pending = []
    for record in records:
        previous = existing.get(record['name'])
        if previous:
            if (previous.get('digest') != f"sha256:{record['sha256']}"
                    or previous['size'] != record['bytes'] or previous.get('state') != 'uploaded'):
                raise ValueError(f"Existing release asset differs; refusing replacement: {record['name']}")
        else:
            pending.append(record['name'])
    return pending


def upload() -> None:
    info = identity()
    release = destination(info)
    folder = OUTPUT / 'uploads'
    records = verify_artifacts(folder, info)
    # Separate hosted receipts leave the curated whole-release manifest/checksums,
    # levels, release notes and guide untouched.
    write_json(folder / 'HOSTED_BUILD_MANIFEST.json', dict(**info, builds=records))
    (folder / 'HOSTED_SHA256SUMS.txt').write_text(''.join(
        f"{record['sha256']}  {record['archive']}\n" for record in records), encoding='utf8')
    names = [r['archive'] for r in records] + ['HOSTED_BUILD_MANIFEST.json', 'HOSTED_SHA256SUMS.txt']
    checks = [dict(name=n, sha256=digest(folder / n), bytes=(folder / n).stat().st_size) for n in names]
    assets = json.loads(run('gh', 'api', '--paginate', '--slurp',
                           f"repos/{info['repository']}/releases/{release['id']}/assets?per_page=100"))
    pending = upload_plan(checks, [a for page in assets for a in page])
    for name in pending:
        current = destination(info)
        if current['id'] != release['id']:
            raise ValueError('Destination release changed during upload')
        # No --clobber: concurrent uploads fail rather than replace anyone's asset.
        run('gh', 'release', 'upload', f"v{info['version']}", folder / name,
            '--repo', info['repository'])
    assets = json.loads(run('gh', 'api', '--paginate', '--slurp',
                           f"repos/{info['repository']}/releases/{release['id']}/assets?per_page=100"))
    if upload_plan(checks, [a for page in assets for a in page]):
        raise ValueError('Upload verification found missing release assets')
    print(f"Uploaded {len(pending)} assets to draft v{info['version']}; draft remains unpublished")


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['preflight', 'package', 'upload'])
    args = parser.parse_args()
    try:
        {'preflight': preflight, 'package': package, 'upload': upload}[args.command]()
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'hosted release: {error}') from error
