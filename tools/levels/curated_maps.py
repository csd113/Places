"""Source and archive views shared by explicit curated-map developer tooling."""
from __future__ import annotations

import json
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[2]
LEGACY_IDS = {'home': 'home_showcase', 'pool': 'pool_showcase', 'office': 'office_demo',
              'beach': 'beach_demo', 'aero': 'frutiger_aero_demo', 'arcade': 'retro_arcade_demo'}


def environment_source(identity: str, root: Path = ROOT) -> Path:
    """Use the editable collection source; support pre-migration checkouts too."""
    source = root / 'projects/environments/levels' / (identity + '.json')
    return source if source.is_file() else root / 'levels' / (LEGACY_IDS.get(identity, identity) + '.json')


def project_manifest(source: Path) -> dict | None:
    document = json.loads(source.read_text())
    return document if document.get('schema') == 'places-project' else None


def source_levels(source: Path) -> list[tuple[str, Path]]:
    """Resolve each declared editable level, including a project's entry map."""
    project = project_manifest(source)
    if project is None:
        return [(json.loads(source.read_text())['id'], source)]
    rows = []
    for record in project['levels']:
        relative = Path(record['source'])
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError(f'{source} has an unsafe level source: {relative}')
        path = source.parent / relative
        if json.loads(path.read_text())['id'] != record['id']:
            raise ValueError(f'{path} does not author ID {record["id"]}')
        rows.append((record['id'], path))
    identities = [identity for identity, _ in rows]
    if not identities or len(identities) != len(set(identities)) or project['entry_level'] not in identities:
        raise ValueError(f'{source} must declare unique levels and an entry level')
    return rows


def lineup(root: Path = ROOT) -> list[tuple[str, Path]]:
    """Resolve only allowlisted distributions; never discover review projects."""
    identities = (root / 'levels/lineup.txt').read_text().splitlines()
    if not identities or len(identities) != len(set(identities)):
        raise ValueError('levels/lineup.txt must contain unique distribution IDs')
    rows = []
    standalone = set()
    for identity in identities:
        if not identity or not all(c.isascii() and (c.isalnum() or c == '_') for c in identity):
            raise ValueError(f'Invalid curated distribution ID: {identity!r}')
        candidates = [root / 'levels' / (identity + '.json'),
                      root / 'projects' / identity / 'places.project.json']
        sources = [path for path in candidates if path.is_file()]
        if len(sources) != 1:
            raise ValueError(f'{identity} must have exactly one maintained source: {candidates}')
        source = sources[0]
        if json.loads(source.read_text())['id'] != identity:
            raise ValueError(f'{source} does not author ID {identity}')
        source_levels(source)
        if source.parent == root / 'levels':
            standalone.add(identity)
        rows.append((identity, source))
    actual = {path.stem for path in (root / 'levels').glob('*.json')}
    if actual != standalone:
        raise ValueError(f'Curated standalone sources differ from lineup: {sorted(actual ^ standalone)}')
    return rows


def level_views(archive: zipfile.ZipFile) -> list[tuple[str, dict[str, str]]]:
    """Map each level's virtual ordinary-package names to physical ZIP records."""
    names = archive.namelist()
    if 'project.json' not in names:
        manifest = json.loads(archive.read('manifest.json'))
        return [(manifest['id'], {name: name for name in names})]
    index = json.loads(archive.read('project.json'))
    if index['schema'] != 'places-project-package' or index['package_format'] != 1:
        raise ValueError('Unsupported project package index')
    expected = [level['id'] for level in index['project']['levels']]
    actual = [level['id'] for level in index['levels']]
    if not expected or actual != expected or len(actual) != len(set(actual)):
        raise ValueError('Project package levels differ from its declared sources')
    return [(level['id'], level['records']) for level in index['levels']]


def validate_lighting(manifest: dict, package: Path) -> None:
    variants = {variant['lightmap_quality']: variant for variant in manifest['variants']}
    if set(variants) != {'off', 'full'}:
        raise ValueError(f'{package} does not contain both Static and Full lighting variants')
    full = variants['full']
    if full.get('lightmap_failure') or not full.get('entries', {}).get('lightmaps'):
        raise ValueError(f'{package} Full lighting fell back instead of retaining its baked atlas')
