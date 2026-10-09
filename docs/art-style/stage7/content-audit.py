#!/usr/bin/env python3
"""Read-only Stage 7 content checks; write owned audit/inventory receipts only."""
from __future__ import annotations

import ast
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
ENTRY = ROOT / "debug-maps/art-style-hero/evidence/stage7-entry/content"
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tools/props"))
import glb

spec = importlib.util.spec_from_file_location("asset_validator", ROOT / "tools/assets/validate.py")
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)
catalog_path = ROOT / "assets/catalog.json"
catalog = json.loads(catalog_path.read_text())
assets = {entry["id"]: entry for entry in catalog["assets"]}
ADOPTED = {
    "home:coffee_table": (.72, 0), "home:dining_table": (.72, 0),
    "home:dining_chair_refined": (.72, 0), "core:pool_table": (.78, 0),
    "core:pool_chair": (.78, 0), "core:pool_ladder": (.48, .65),
    **{f"winter:{name}": (.55, 0) for name in (
        "icicle_short", "icicle_medium", "icicle_long", "icicle_cluster_mixed",
        "icicle_cluster_sparse")},
}


def sha(payload):
    return hashlib.sha256(payload).hexdigest()


def write(name, data):
    (OUT / name).write_text(json.dumps(data, indent=2) + "\n")


def response(document, index):
    material = document.get("materials", [{}])[index or 0]
    pbr = material.get("pbrMetallicRoughness", {})
    return {
        "base_color": pbr.get("baseColorFactor", [1, 1, 1, 1]),
        "texture": pbr.get("baseColorTexture"),
        "emission": material.get("emissiveFactor", [0, 0, 0]),
        "extensions": material.get("extensions", {}),
        "alpha_mode": material.get("alphaMode", "OPAQUE"),
        "alpha_cutoff": material.get("alphaCutoff", .5),
        "double_sided": material.get("doubleSided", False),
    }


def function_source(path, name):
    source = path.read_text()
    node = next(n for n in ast.parse(source).body if isinstance(n, ast.FunctionDef) and n.name == name)
    return ast.get_source_segment(source, node)


model_rows = []
for asset_id, values in ADOPTED.items():
    path = Path("assets") / assets[asset_id]["model"]
    before_bytes = (ENTRY / path).read_bytes()
    after_bytes = (ROOT / path).read_bytes()
    before = glb.read_glb(before_bytes)
    after = glb.read_glb(after_bytes)
    equal = {key: getattr(before, key) == getattr(after, key) for key in
             ("positions", "uvs", "colors", "indices", "texture_pngs")}
    equal["bounds"] = before.bounds() == after.bounds()
    equal["non_scalar_triangle_material_response"] = all(
        response(before.json, old) == response(after.json, new)
        for old, new in zip(before.triangle_materials, after.triangle_materials))
    equal["triangle_material_count"] = len(before.triangle_materials) == len(after.triangle_materials)
    assert all(equal.values()), (asset_id, equal)
    materials = []
    for index, material in enumerate(after.json["materials"]):
        pbr = material.get("pbrMetallicRoughness", {})
        materials.append({"slot": index, "name": material.get("name"),
                          "used_triangles": after.triangle_materials.count(index),
                          "roughness": pbr.get("roughnessFactor", 1),
                          "metallic": pbr.get("metallicFactor", 1)})
    assert any(m["used_triangles"] and (m["roughness"], m["metallic"]) == values for m in materials)
    model_rows.append({"id": asset_id, "path": str(path),
                       "before_sha256": sha(before_bytes), "after_sha256": sha(after_bytes),
                       "before_bytes": len(before_bytes), "after_bytes": len(after_bytes),
                       "triangles": after.triangle_count, "vertices": len(after.positions),
                       "exact_equal": equal, "materials": materials,
                       "embedded_png_sha256": [sha(p) for p in after.texture_pngs]})

chair = Path("assets") / assets["home:dining_chair"]["model"]
original_chair_equal = (ENTRY / chair).read_bytes() == (ROOT / chair).read_bytes()
original_chair_function_equal = function_source(ENTRY / "tools/props/parts/home_remade.py", "dining_chair") == function_source(ROOT / "tools/props/parts/home_remade.py", "dining_chair")
catalog_equal = (ENTRY / "assets/catalog.json").read_bytes() == catalog_path.read_bytes()
assert original_chair_equal and original_chair_function_equal and catalog_equal
source_preservation = []
for folder in ("assets/levels", "levels", "tests/fixtures/levels"):
    for preserved in sorted((ENTRY / folder).rglob("*.json")):
        relative = preserved.relative_to(ENTRY)
        equal = preserved.read_bytes() == (ROOT / relative).read_bytes()
        assert equal, relative
        source_preservation.append({"path": str(relative), "sha256": sha(preserved.read_bytes()), "byte_identical": equal})
write("content-scalar-adoption.json", {
    "format_version": 1, "models": model_rows,
    "original_dining_chair_asset_byte_identical": original_chair_equal,
    "original_dining_chair_function_exact_source_identical": original_chair_function_equal,
    "catalog_byte_identical": catalog_equal,
    "existing_source_preservation": source_preservation,
    "collision_contract": "Catalog sizes/solid flags and all existing level placement/scale/size/rotation/solid/occludes fields are byte-identical. Decoded model bounds and geometry are exact.",
    "container_change": "Existing Mesh.material API introduces named family slots/primitive material groups and toolkit metadata where needed. Equivalent explicit material defaults preserve colour/alpha/emission. Only roughness/metallic response changes on the approved faces.",
    "deliberate_defaults": "Original dining chair is the immutable comparison asset. Fabrics, snow, unfinished/natural stocks and unselected model families retain their existing matte defaults. Pool ladder rubber remains roughness1/metallic0. Icicle snow slot remains unchanged. No normalTexture/tangent support is invented.",
})

generated = {
    "model_zoo": "python3 tools/levels/build_model_zoo.py",
    "lantern_hollow": "python3 tools/levels/build_lantern_hollow.py",
    "winter": "python3 tools/levels/build_winter.py",
    "capacity_dense": "python3 tools/levels/build_capacity_fixtures.py",
    "capacity_beyond_former_limits": "python3 tools/levels/build_capacity_fixtures.py",
    "outdoor_kit_showcase": "python3 tools/levels/build_outdoor_fixture.py",
    "lighting_quality": "python3 tools/levels/build_lighting_quality.py",
    "prop_showcase": "python3 tools/levels/build_fixture_levels.py",
    "prop_stress": "python3 tools/levels/build_fixture_levels.py",
}


def references(value):
    result = set()
    if isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]+:[A-Za-z0-9_-]+", value):
        result.add(value)
    elif isinstance(value, dict):
        for child in value.values():
            result.update(references(child))
    elif isinstance(value, list):
        for child in value:
            result.update(references(child))
    return result


def dependencies(ids):
    files = set()
    seen = set()
    def visit(asset_id):
        if asset_id in seen or asset_id not in assets:
            return
        seen.add(asset_id)
        entry = assets[asset_id]
        if entry.get("model"):
            files.add("assets/" + entry["model"])
        for child in references(entry):
            visit(child)
    for asset_id in ids:
        visit(asset_id)
    return sorted(files)


rows = []
sources = sorted([*(ROOT / "assets/levels").glob("*.json"),
                  *(ROOT / "levels").glob("*.json"),
                  *(ROOT / "tests/fixtures/levels").rglob("*.json")])
for path in sources:
    level = json.loads(path.read_text())
    relative = str(path.relative_to(ROOT))
    key = path.stem
    if relative.startswith("assets/levels/"):
        classification = "shipped-supported"
    elif key in ("blizzard_review", "snowfall_contrast") and relative.startswith("levels/"):
        classification = "local-supported-recovered-serialized-source"
    elif relative.startswith("levels/"):
        classification = "local-supported-historical-control"
    elif "/invalid/" in relative:
        classification = "intentional-loader-rejection-control"
    elif key == "geometry_broken":
        classification = "intentional-geometry-defect-control"
    elif "/repair/" in relative:
        classification = "geometry-repair-control"
    elif key.startswith("art_style_hero"):
        classification = "historical-milestone-or-additive-hero-control"
    elif key.startswith("capacity_"):
        classification = "generated-capacity-regression"
    else:
        classification = "maintained-regression-fixture"
    refs = references(level)
    # The three unnamespaced canonical entity IDs also occur in these fields.
    refs.update(asset_id for asset_id, _ in validator.level_ids(level) if asset_id in assets)
    missing = sorted(refs - assets.keys())
    synthetic = key == "capacity_beyond_former_limits"
    assert not missing or synthetic, (relative, missing[:10])
    generation = generated.get(key) if not relative.startswith("levels/") else None
    if generation:
        authoring = {"mode": "generated", "command": generation,
                     "check_command": generation + " --check" if key not in ("prop_showcase", "prop_stress") else None}
    elif key == "places_demo":
        authoring = {"mode": "maintained-composite-source", "command": None,
                     "slice_generators": ["tools/levels/build_outdoor_route.py", "tools/levels/refine_home.py", "tools/levels/refine_pool.py"],
                     "note": "These tools own slices, not a complete regeneration. Preserve other source arrays and gameplay."}
    elif key in ("blizzard_review", "snowfall_contrast") and relative.startswith("levels/"):
        authoring = {"mode": "recovered-serialized-LevelDef", "command": None,
                     "provenance": "content-recovered-local-sources.json", "note": "Identity extraction of entry package semantics, not original pre-prepare source bytes."}
    else:
        authoring = {"mode": "maintained-authored-control", "command": None,
                     "note": "No generator owns this source; preserve its explicit test/control intent."}
    output = relative.removesuffix(".json") + ".placesmap" if classification.startswith(("shipped-", "local-")) else "debug-maps/art-style-hero/evidence/stage7-fixture-packages/" + relative.removeprefix("tests/fixtures/levels/").removesuffix(".json") + ".placesmap"
    expected = "reject" if "loader-rejection" in classification else "accept"
    compile_command = f"./target/release/places-compile build {relative} --out {output} --variants off,medium,full --workers 12"
    validation = {"loader_expectation": expected,
                  "compile_command": compile_command,
                  "package_validate_command": f"./target/release/places-compile validate {output}",
                  "package_current_command": f"./target/release/places-compile verify {relative} --package {output} --require-current",
                  "geometry_command": f"./target/release/places --check-geometry --level {relative}",
                  "note": "Commands are planned primary/validation checks, not executed by content owner. Compile/validate commands for intentional invalid source must reject; defect and repair controls retain expected geometry findings. Native views require compiled packages and isolated discovery roots."}
    source_entry = ENTRY / relative
    affected = sorted(refs & ADOPTED.keys())
    row = {"source": relative, "id": level["id"], "name": level["name"],
           "format_version": level["format_version"], "classification": classification,
           "source_bytes": path.stat().st_size, "source_sha256": sha(path.read_bytes()),
           "existing_entry_byte_identical": source_entry.read_bytes() == path.read_bytes() if source_entry.exists() else None,
           "counts": {k: len(v) for k, v in level.items() if isinstance(v, list)},
           "spawn": level["spawn"], "authoring": authoring, "validation": validation,
           "catalog_reference_count": len(refs & assets.keys()), "catalog_references": sorted(refs & assets.keys()),
           "dependency_files": dependencies(refs), "scalar_affected_models": affected,
           "synthetic_catalog_exception": {"explicit_existing_exception": True,
                "unknown_id_count": len(missing), "examples": missing[:8],
                "authority": "tools/assets/validate.py GENERATED_CAPACITY_FIXTURES; src/assets/tests.rs; src/zoo_audit.rs"} if synthetic else None,
           "preservation_decision": "Retain exact historical control semantics; refreshed package data uses current supported contracts."}
    rows.append(row)
write("content-inventory.json", {
    "format_version": 1, "entry_revision": "26486b8538424f013c243ae6edea8720ac07d7f2",
    "inventory_scope": "Every JSON source recursively under shipped assets/levels, local levels and tests/fixtures/levels; ignored local review packages recovered to exact serialized sources. Historical debug-maps snapshots are immutable evidence, not additional maintained maps.",
    "source_count": len(rows), "shipped_count": 5, "local_count": 5, "fixture_count": 40,
    "catalog_format_version": catalog["format_version"], "catalog_asset_count": len(assets),
    "catalog_sha256": sha(catalog_path.read_bytes()),
    "runtime_discovery": "src/loader.rs discovers .placesmap packages only; raw JSON remains authoring/checker input. Fixture variants sharing art_style_hero or local/fixture home_showcase IDs require separate package roots.",
    "validation_scope": "Rust compile/loader is authoritative; Python asset validator is complementary and its stock top-level scan intentionally omits nested invalid/repair controls. No new skip/filter is added.",
    "historical_control_decisions": {
        "levels/home_showcase.json": "Keep valid core furniture architecture/control, distinct from completed Home fixture. Rebuild stale package; no forced model swap.",
        "tests/fixtures/levels/prop_showcase.json": "Fixed 39 core placements, including legal sunk and overlapping instances. Not an all-catalog showroom; Model Zoo owns that role.",
        "tests/fixtures/levels/prop_stress.json": "Fixed 152-instance batching witness. Preserve model/instance distribution.",
        "art_style_hero variants": "Original and staged/additive sources preserve benchmark/control roles, including original chair. Refined source remains additive.",
        "invalid and repair": "Keep degenerate loader rejection and planted geometry/repair cases unchanged. Validation must classify expected failures rather than remove them.",
        "capacity_sparse.json": "Retired and absent; maintained capacity_dense and capacity_beyond_former_limits are actual supported generated fixtures.",
        "model_zoo.json": "All 218 current placements unchanged, including Stage6 additive chair display; all prior217 placements preserved byte for byte by unchanged source.",
    }, "sources": rows,
})
print(f"Audited {len(rows)} sources, {len(model_rows)} scalar models; every preservation assertion passed.")
