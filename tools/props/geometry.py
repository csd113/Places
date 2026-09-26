"""Conservative winding repair and seam-aware topology audit for static props.

Position welding is used only to discover adjacency; UV/color seam vertices are
never merged. Non-manifold edges and zero-volume sheets are not treated as solids.
"""
from collections import defaultdict, deque


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _cross(a, b):
    return (a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0])


def _dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def inspect(positions, indices, *, repair=False):
    """Orient edge-connected shells consistently, then outward by signed volume.

    Open curved shells use their centred signed volume; planar sheets keep their
    majority orientation. Contradictory/non-orientable components are left alone.
    The caller owns intentional openings: this function never fills holes.
    """
    keys = [tuple(round(v, 6) for v in p) for p in positions]
    triangles = [indices[i:i+3] for i in range(0, len(indices), 3)]
    edges = defaultdict(list)
    degenerate = 0
    for face, tri in enumerate(triangles):
        a, b, c = [positions[i] for i in tri]
        normal = _cross(_sub(b, a), _sub(c, a))
        degenerate += _dot(normal, normal) < 1e-20
        for i in range(3):
            a, b = keys[tri[i]], keys[tri[(i+1)%3]]
            edges[tuple(sorted((a, b)))].append((face, a < b))
    graph = defaultdict(list)
    conflicts = 0
    for uses in edges.values():
        if len(uses) == 2:
            (a, da), (b, db) = uses
            graph[a].append((b, da == db))
            graph[b].append((a, da == db))
            conflicts += da == db
    visited = set()
    flipped = []
    shells = []
    for start in range(len(triangles)):
        if start in visited:
            continue
        signs = {start: False}
        queue = deque([start])
        contradiction = False
        while queue:
            a = queue.popleft()
            for b, toggle in graph[a]:
                value = signs[a] ^ toggle
                if b in signs:
                    contradiction |= signs[b] != value
                else:
                    signs[b] = value
                    queue.append(b)
        visited.update(signs)
        points = {tuple(positions[v]) for face in signs for v in triangles[face]}
        center = tuple(sum(p[i] for p in points)/len(points) for i in range(3))
        volume = 0.0
        for face, flip in signs.items():
            a, b, c = [_sub(positions[v], center) for v in triangles[face]]
            volume += _dot(a, _cross(b, c)) * (-1 if flip else 1) / 6
        extent = max(max(p[i] for p in points)-min(p[i] for p in points) for i in range(3))
        nonplanar = abs(volume) > max(1e-14, extent**3 * 1e-10)
        invert = volume < 0 if nonplanar else sum(signs.values()) > len(signs)/2
        selected = [] if contradiction else [face for face, value in signs.items() if value ^ invert]
        flipped.extend(selected)
        shells.append({'triangles':len(signs), 'volume':volume, 'flips':len(selected), 'contradiction':contradiction})
    if repair:
        for face in flipped:
            offset = face*3
            indices[offset+1], indices[offset+2] = indices[offset+2], indices[offset+1]
    return {'triangles':len(triangles), 'degenerate':degenerate,
            'boundary_edges':sum(len(v)==1 for v in edges.values()),
            'nonmanifold_edges':sum(len(v)>2 for v in edges.values()),
            'inconsistent_edges':conflicts, 'flipped_triangles':len(flipped),
            'components':len(shells), 'contradictory_components':sum(s['contradiction'] for s in shells)}


def boundary_loops(positions, indices):
    """Return simple boundary cycles; ambiguous junctions are deliberately skipped."""
    keys = [tuple(round(v, 6) for v in p) for p in positions]
    edges = defaultdict(list)
    for offset in range(0, len(indices), 3):
        tri = indices[offset:offset+3]
        for a, b in zip(tri, tri[1:]+tri[:1]):
            edges[tuple(sorted((keys[a], keys[b])))].append((a, b, offset))
    boundary = [uses[0] for uses in edges.values() if len(uses)==1]
    neighbors = defaultdict(list)
    for i, (a, b, _) in enumerate(boundary):
        neighbors[keys[a]].append(i); neighbors[keys[b]].append(i)
    used = set(); loops = []
    for start in range(len(boundary)):
        if start in used:
            continue
        a, b, _ = boundary[start]
        loop = [a]; current = start; point = keys[a]; cycle = set()
        while current not in cycle:
            cycle.add(current); used.add(current)
            a, b, _ = boundary[current]
            nxt = b if keys[a]==point else a
            point = keys[nxt]
            if point == keys[loop[0]]:
                loops.append((loop, [boundary[e][2] for e in cycle])); break
            loop.append(nxt)
            if len(neighbors[point]) != 2:
                break
            current = next(e for e in neighbors[point] if e != current)
    return loops
