"""Offline Blender exact-union step for the rat; retains corner UVs and colours.

Blender is an authoring dependency only. The game loads the finished GLB.
"""
from pathlib import Path
import json
import shutil
import subprocess
import sys
import tempfile


def union_surface(mesh, parts):
    from mesh import Mesh
    blender = shutil.which('blender')
    if blender is None:
        raise RuntimeError('Rebuilding rat geometry requires Blender on PATH')
    with tempfile.TemporaryDirectory(prefix='places-rat-') as temporary:
        path = Path(temporary) / 'surface.json'
        path.write_text(json.dumps(dict(positions=mesh.positions, indices=mesh.indices,
                                       uvs=mesh.uvs, colors=mesh.colors, parts=parts)))
        subprocess.run([blender, '--background', '--factory-startup', '--python-exit-code', '1', '--python',
                        str(Path(__file__).resolve()), '--', str(path)], check=True,
                       stderr=subprocess.STDOUT)
        data = json.loads(path.read_text())
    result = Mesh()
    for field in ('positions', 'indices', 'uvs', 'colors'):
        setattr(result, field, data[field])
    return result, data['parts']


def _blender_union(path):
    import bpy
    import bmesh
    data = json.loads(path.read_text())
    bpy.ops.object.select_all(action='SELECT')
    bpy.ops.object.delete(use_global=False)
    materials = [bpy.data.materials.new(name) for name in data['parts']]
    objects = []
    for part_index, (name, (start, end)) in enumerate(data['parts'].items()):
        triangles = [data['indices'][i:i + 3] for i in range(0, len(data['indices']), 3)
                     if start <= data['indices'][i] < end]
        mesh = bpy.data.meshes.new(name)
        mesh.from_pydata([[c * 100 for c in p] for p in data['positions'][start:end]], [],
                         [[v - start for v in tri] for tri in triangles])
        for material in materials:
            mesh.materials.append(material)
        uv = mesh.uv_layers.new()
        color = mesh.color_attributes.new(name='Color', type='FLOAT_COLOR', domain='CORNER')
        for face in mesh.polygons:
            face.material_index = part_index
            for loop in face.loop_indices:
                source = mesh.loops[loop].vertex_index + start
                uv.data[loop].uv = data['uvs'][source]
                color.data[loop].color = [c / 255 for c in data['colors'][source]] + [1]
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=1e-5)
        bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
        bm.to_mesh(mesh)
        bm.free()
        obj = bpy.data.objects.new(name, mesh)
        bpy.context.collection.objects.link(obj)
        objects.append(obj)
    result = objects[0]
    bpy.context.view_layer.objects.active = result
    for other in objects[1:]:
        modifier = result.modifiers.new('Weld anatomical junction', 'BOOLEAN')
        modifier.operation = 'UNION'
        modifier.solver = 'EXACT'
        modifier.object = other
        bpy.ops.object.modifier_apply(modifier=modifier.name)
        bpy.data.objects.remove(other, do_unlink=True)
    bm = bmesh.new()
    bm.from_mesh(result.data)
    bmesh.ops.remove_doubles(bm, verts=list(bm.verts), dist=1e-5)
    bmesh.ops.dissolve_limit(bm, angle_limit=1e-5, verts=list(bm.verts),
                             edges=list(bm.edges), delimit={'UV', 'MATERIAL'})
    bmesh.ops.triangulate(bm, faces=list(bm.faces))
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    if any(not edge.is_manifold for edge in bm.edges):
        raise RuntimeError('Rat union has non-manifold edges')
    remaining = set(bm.verts)
    components = 0
    while remaining:
        components += 1
        pending = [remaining.pop()]
        while pending:
            vertex = pending.pop()
            for edge in vertex.link_edges:
                neighbor = edge.other_vert(vertex)
                if neighbor in remaining:
                    remaining.remove(neighbor)
                    pending.append(neighbor)
    if components != 1:
        raise RuntimeError(f'Rat union has {components} disconnected shells')
    bm.to_mesh(result.data)
    bm.free()
    mesh = result.data
    output = dict(positions=[], indices=[], uvs=[], colors=[], parts={})
    for part_index, name in enumerate(data['parts']):
        start = len(output['positions'])
        lookup = {}
        for face in mesh.polygons:
            if face.material_index != part_index:
                continue
            for loop in face.loop_indices:
                point = tuple(c / 100 for c in mesh.vertices[mesh.loops[loop].vertex_index].co)
                uv = tuple(mesh.uv_layers.active.data[loop].uv)
                color = tuple(round(c * 255) for c in mesh.color_attributes['Color'].data[loop].color[:3])
                key = point, uv, color
                if key not in lookup:
                    lookup[key] = len(output['positions'])
                    output['positions'].append(point)
                    output['uvs'].append(uv)
                    output['colors'].append(color)
                output['indices'].append(lookup[key])
        output['parts'][name] = start, len(output['positions'])
    path.write_text(json.dumps(output))


if __name__ == '__main__':
    _blender_union(Path(sys.argv[sys.argv.index('--') + 1]))


def surface_checks(positions, indices, joints, weights):
    """Audit geometric topology across intentional GLB UV/colour splits."""
    canonical = {}
    vertices = []
    influences = {}
    consistent_weights = True
    for index, point in enumerate(positions):
        key = tuple(round(value, 7) for value in point)
        vertex = canonical.setdefault(key, len(canonical))
        vertices.append(vertex)
        skin = tuple(sorted((joint, round(weight, 7)) for joint, weight
                            in zip(joints[index], weights[index]) if weight > 0))
        if vertex in influences and influences[vertex] != skin:
            consistent_weights = False
        influences[vertex] = skin
    edges = {}
    adjacency = {vertex: set() for vertex in vertices}
    degenerate = 0
    volume = 0.0
    for offset in range(0, len(indices), 3):
        source = indices[offset:offset + 3]
        triangle = [vertices[index] for index in source]
        a, b, c = [positions[index] for index in source]
        ab = [b[i] - a[i] for i in range(3)]
        ac = [c[i] - a[i] for i in range(3)]
        cross = (ab[1]*ac[2] - ab[2]*ac[1], ab[2]*ac[0] - ab[0]*ac[2],
                 ab[0]*ac[1] - ab[1]*ac[0])
        degenerate += len(set(triangle)) != 3 or sum(v*v for v in cross) < 1e-24
        volume += sum(a[i] * cross[i] for i in range(3)) / 6
        for x, y in zip(triangle, triangle[1:] + triangle[:1]):
            edges.setdefault(tuple(sorted((x, y))), []).append((x, y))
            adjacency[x].add(y)
            adjacency[y].add(x)
    remaining = set(adjacency)
    components = 0
    while remaining:
        components += 1
        pending = [remaining.pop()]
        while pending:
            for neighbor in adjacency[pending.pop()]:
                if neighbor in remaining:
                    remaining.remove(neighbor)
                    pending.append(neighbor)
    return {
        'closed manifold surface': all(len(faces) == 2 for faces in edges.values()),
        'consistent outward winding': volume > 0 and all(
            len(faces) == 2 and faces[0] == faces[1][::-1] for faces in edges.values()),
        'one connected anatomical shell': components == 1,
        'no degenerate surface triangles': degenerate == 0,
        'skin weights agree across UV seams': consistent_weights,
    }
