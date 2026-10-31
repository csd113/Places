#!/usr/bin/env python3
"""Qualify explicit native character captures using actual pose/clip receipts.

Never bakes or launches a map. --expected describes level, drawable, captures
(name, frame, actors: id/clip/paused/opacity interval), and optional series
(id, captures, pose_changed/clock_wrap/fade_reversal). Frame captures must carry
the player's capture-time character_animations and genuine final draw receipt.
Exact opacity [0, 0] also requires no socket light and an image comparison.
The mesh registration flag may remain true at zero opacity. --equal
compares two real PNGs exactly (e.g. hidden versus a separately captured absent
control). No clip, zero opacity, or pose is inferred from a filename.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path


def finite(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def inspect(log: str, expected: dict) -> list[dict]:
    if any(marker in log.lower() for marker in ('capture failed', 'applied=false', 'native lighting sequence action failed')):
        raise ValueError('Native capture/action failed')
    captures = expected['captures']
    if not 1 <= len(captures) <= 64:
        raise ValueError('Expected 1..64 captures')
    states, draws, pending = [], [], None
    for line in log.splitlines():
        if line.startswith('[lighting-capture-state] '):
            if pending is not None:
                raise ValueError('Capture state has no draw receipt')
            pending = json.loads(line.removeprefix('[lighting-capture-state] '))
        elif line.startswith('[visual-diagnostic] '):
            draw = json.loads(line.removeprefix('[visual-diagnostic] '))
            if draw.get('event') == 'capture':
                if pending is None:
                    raise ValueError('Draw has no capture-time character state')
                states.append(pending); draws.append(draw); pending = None
    if pending is not None or len(states) != len(captures):
        raise ValueError('Capture count/state mismatch')
    result = []
    for state, draw, spec in zip(states, draws, captures):
        if (draw.get('mode') != 'final' or draw.get('level') != expected['level']
                or draw['frame'].get('drawable') != expected['drawable']
                or draw['frame'].get('diagnostic_identity_post') is not False
                or draw['capture_submission'].get('draw_calls', 0) <= 0
                or draw['capture_submission'].get('submitted_indices', 0) <= 0):
            raise ValueError('Wrong world/drawable or no final scene submission')
        if state.get('ready_frame') != spec['frame'] or Path(state['capture_path']).stem != spec['name']:
            raise ValueError('Capture identity/frame mismatch')
        actors = state.get('character_animations')
        requested = spec['actors']
        if not isinstance(actors, list) or len(actors) > 256 or len(requested) > 32:
            raise ValueError('Missing or oversized character receipt')
        if spec.get('absent') and actors:
            raise ValueError('Absent control contains characters')
        checked = []
        for request in requested:
            matches = [actor for actor in actors if actor.get('id') == request['id']]
            if len(matches) != 1:
                raise ValueError('Character identity missing/ambiguous')
            actor = matches[0]; cue = actor.get('cue') or {}
            if cue.get('clip') != request['clip'] or cue.get('paused') is not request['paused']:
                raise ValueError(f"{request['id']}: actual selected clip/paused mismatch")
            clock, duration, opacity = cue.get('time'), cue.get('duration'), actor.get('opacity')
            if (not all(finite(v) for v in (clock, duration, opacity)) or duration <= 0
                    or not 0 <= clock <= duration or not 0 <= opacity <= 1
                    or not request['opacity'][0] <= opacity <= request['opacity'][1]):
                raise ValueError('Invalid actual animation clock/opacity')
            if request['opacity'] == [0, 0] and (not isinstance(actor.get('mesh_visible'), bool)
                    or 'light_position' not in actor or actor['light_position'] is not None):
                raise ValueError('Exact-zero character lacks registration evidence or retains light')
            bounds, origin = actor.get('posed_bounds', {}), actor.get('origin', [])
            if (len(origin) != 3 or any(len(bounds.get(key, [])) != 3 for key in ('min', 'max'))
                    or not all(finite(v) for v in origin + bounds['min'] + bounds['max'])
                    or any(a > b for a, b in zip(bounds['min'], bounds['max']))):
                raise ValueError('Missing/invalid actual posed bounds')
            checked.append({key: actor.get(key) for key in
                            ('id', 'cue', 'opacity', 'posed_bounds', 'origin', 'mesh_visible', 'light_position')})
        result.append(dict(name=spec['name'], frame=spec['frame'], path=state['capture_path'], actors=checked))
    names = [row['name'] for row in result]
    if len(set(names)) != len(names):
        raise ValueError('Duplicate capture identity')
    for series in expected.get('series', []):
        rows = [next(row for row in result if row['name'] == name) for name in series['captures']]
        if len(rows) < 2 or any(a['frame'] >= b['frame'] for a, b in zip(rows, rows[1:])):
            raise ValueError('Series must be chronological')
        actors = [next(actor for actor in row['actors'] if actor['id'] == series['id']) for row in rows]
        clocks = [actor['cue']['time'] for actor in actors]
        if (len(set(clocks)) < 2 or any(actor['cue']['paused'] for actor in actors)
                or len({(actor['cue']['clip'], actor['cue']['duration']) for actor in actors}) != 1):
            raise ValueError('Series must advance the same unpaused clip')
        if series.get('clock_wrap') and not any(a > b for a, b in zip(clocks, clocks[1:])):
            raise ValueError('No actual clock wrap')
        poses = [[value - origin for key in ('min', 'max')
                  for value, origin in zip(actor['posed_bounds'][key], actor['origin'])] for actor in actors]
        if series.get('pose_changed') and not any(abs(a-b) > 1e-5 for pose in poses[1:] for a, b in zip(poses[0], pose)):
            raise ValueError('No posed change beyond origin translation')
        opacities = [actor['opacity'] for actor in actors]
        if series.get('fade_reversal') and not (any(a > b for a, b in zip(opacities, opacities[1:]))
                                                and any(a < b for a, b in zip(opacities, opacities[1:]))):
            raise ValueError('No actual fade reversal')
    return result


def read_bounded(path: Path, limit: int) -> str:
    with path.open('rb') as stream:
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise ValueError(f'Oversized input: {path}')
    return data.decode('utf8')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--log', type=Path, required=True)
    parser.add_argument('--expected', type=Path, required=True)
    parser.add_argument('--equal', type=Path, nargs=2, action='append', default=[])
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    expected_text = read_bounded(args.expected, 1024 * 1024)
    log = read_bounded(args.log, 32 * 1024 * 1024)
    expected = json.loads(expected_text)
    captures = inspect(log, expected)
    zero_paths = {Path(capture['path']).resolve() for capture, spec in zip(captures, expected['captures'])
                  if any(actor['opacity'] == [0, 0] for actor in spec['actors'])}
    compared_paths = set()
    from PIL import Image, ImageChops
    for capture in captures:
        path = Path(capture['path'])
        with Image.open(path) as image:
            if image.format != 'PNG' or list(image.size) != json.loads(expected_text)['drawable']:
                raise ValueError('Missing/wrong native PNG')
            image.verify()
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            for block in iter(lambda: stream.read(1024 * 1024), b''):
                digest.update(block)
        capture['sha256'] = digest.hexdigest()
    pairs = []
    for left, right in args.equal:
        if left.resolve() == right.resolve():
            raise ValueError('Comparison requires distinct native captures')
        compared_paths.update((left.resolve(), right.resolve()))
        with Image.open(left) as a, Image.open(right) as b:
            if a.size != b.size or ImageChops.difference(a.convert('RGB'), b.convert('RGB')).getbbox() is not None:
                raise ValueError('Native images differ')
        pairs.append(dict(left=str(left), right=str(right), equal_rgb=True))
    if not zero_paths <= compared_paths:
        raise ValueError('Exact-zero acceptance requires an explicit hidden/absent image comparison')
    with args.out.open('x', encoding='utf8') as stream:
        json.dump(dict(passed=True, captures=captures, equal_images=pairs,
                       log_sha256=hashlib.sha256(log.encode()).hexdigest(),
                       expected_sha256=hashlib.sha256(expected_text.encode()).hexdigest()), stream, indent=2)
        stream.write('\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
