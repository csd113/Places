#!/usr/bin/env python3
"""Check loop closure and Spoonerman's transition endpoints in exported GLBs."""
from pathlib import Path
import math
from validate_entities import Model, REPO_ROOT


def endpoints(model, animation, last):
    result = {}
    for channel in animation['channels']:
        sampler = animation['samplers'][channel['sampler']]
        path = channel['target']['path']
        values = model._read_accessor(sampler['output'], 'VEC4' if path == 'rotation' else 'VEC3')
        result[channel['target']['node'], path] = values[-1 if last else 0]
    return result


def difference(a, b, rotation):
    direct = max(abs(x - y) for x, y in zip(a, b))
    return min(direct, max(abs(x + y) for x, y in zip(a, b))) if rotation else direct


def compare(model, a, b, label):
    for key in a.keys() | b.keys():
        node, path = key
        default = model.nodes[node].get(path, [0, 0, 0, 1] if path == 'rotation' else
                                        [1, 1, 1] if path == 'scale' else [0, 0, 0])
        if difference(a.get(key, default), b.get(key, default), path == 'rotation') > 1e-5:
            raise ValueError(f'{label}: discontinuous {model.nodes[node].get("name", node)} {path}')


def main():
    for name in ('rat', 'mannequin', 'skeleton', 'spooner-man'):
        model = Model(REPO_ROOT / f'assets/entities/{name}/model/{name}.glb')
        animations = {a['name']: a for a in model.animations}
        for clip, animation in animations.items():
            if clip not in ('sit_down', 'stand_up'):
                compare(model, endpoints(model, animation, False), endpoints(model, animation, True),
                        f'{name}/{clip} loop')
        for weights in model.vertex_weights:
            if any(not math.isfinite(w) or w < 0 for w in weights) or abs(sum(weights) - 1) > 1e-5:
                raise ValueError(f'{name}: invalid skin weights')
        if name == 'spooner-man':
            for first, last in (('idle', 'sit_down'), ('sit_down', 'sit_idle'),
                                ('sit_idle', 'stand_up'), ('stand_up', 'idle')):
                compare(model, endpoints(model, animations[first], True),
                        endpoints(model, animations[last], False), f'{first} -> {last}')
        print(f'{name}: {len(animations)} clips, loop/transition boundaries and skin weights OK')


if __name__ == '__main__':
    main()
