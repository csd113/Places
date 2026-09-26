"""spooner-man: a low-poly tuxedo cat, built from the pack's primitives.

The canonical id is ``spooner-man`` (hyphen), matching `assets/catalog.json`
and every level that places him; the underscore spelling is only this Python
module's name. This module builds the toolkit's **static** cat: one mesh, one
material, one 256x256 texture. The shipped entity is a hand-authored Blender
export that carries a real 26-joint skin and five authored clips (see
`assets/entities/spooner-man/README.md`); the toolkit refuses to overwrite it
unless `--force` is passed.

Construction notes
------------------
* Torso, neck, head, muzzle, legs and paws are low-segment `lathe` masses with
  explicit ring/segment counts (no decimation), and the tail is a six-span swept
  tube, so the whole cat costs roughly a thousand triangles.
* Texture regions do the tuxedo work. The torso's ``u`` axis runs around the
  body (``0`` spine, ``0.25`` left, ``0.5`` belly, ``0.75`` right) and ``v``
  runs from the chest (top of the region) to the rump, so one region yields the
  black back and sides, the white ventral band and the wider white chest. The
  face region projects the white muzzle, narrow blaze, amber eyes, pink nose
  and dark chin continuously across the front of the skull and muzzle. A
  separate right hind-leg region joins its single white ring to the belly;
  the paw region is white with toe separations.
* Pose: standing with a slight crouch, torso horizontal, head forward, all four
  paws on the ground plane facing +Z, and the tail extending back with a
  gentle rise, as in the standing side-view reference. The reference
  photograph in which the cat is held up vertically is deliberately not the
  pose.
"""

from __future__ import annotations

import math

import palette
from mesh import PropBuilder
from tex import Texture

# Faded reference colours, chosen to stay readable at 480x272.
FUR_BLACK = (38, 38, 42)
FUR_BLACK_WARM = (54, 50, 54)
WHITE_FUR = (232, 229, 221)
WHITE_SHADE = (208, 204, 194)
PINK_NOSE = (198, 140, 134)
EYE_AMBER = (216, 162, 58)
EYE_AMBER_DARK = (178, 124, 38)
PUPIL = (26, 22, 20)
# Geometry tints the (already coloured) texture, so geometry uses a neutral tint.
COAT_TINT = (255, 255, 255)
WHISKER = (176, 172, 164)

# Body layout in metres (origin: floor-contact centre, +Z is the front).
TORSO_REAR_Z = -0.320
TORSO_CENTRE_Y = 0.225
HEAD_BASE_Z = 0.170
HEAD_TIP_Z = 0.370
MUZZLE_BASE_Z = 0.302
FRONT_LEG_X = 0.062
REAR_LEG_X = 0.070
PAW_FRONT_Z = 0.132
PAW_REAR_Z = -0.238

# Explicit texture layout on the 256x256 canvas: more pixels around the torso
# and across the face (where the markings are), fewer on flat black parts.
REGIONS = {
    "body": (0, 0, 128, 96),
    "head": (128, 0, 128, 64),
    "muzzle": (0, 96, 64, 48),
    "paw": (64, 96, 64, 48),
    "leg": (128, 64, 32, 32),
    "neck": (224, 64, 32, 32),
    "tail": (160, 64, 32, 32),
    "ear": (192, 64, 32, 32),
    "ring_leg": (64, 144, 64, 64),
    "face": (128, 144, 128, 112),
}


def _paint_coat(tex: Texture) -> None:
    """Paints the whole tuxedo coat, face and socks."""
    tex.fill("body", FUR_BLACK, jitter=7, seed=41)
    tex.noise("body", amount=6, freq=3, seed=42)
    tex.streaks("body", FUR_BLACK_WARM, count=9, seed=43, alpha=46)

    # White ventral band: narrow at the rump, widening into a full chest bib.
    rows = tex.cell("body")[3]
    for row in range(rows):
        t = row / max(1, rows - 1)  # 0 = chest (top of the region), 1 = rump
        half_width = 0.095 + 0.155 * (1.0 - t) ** 1.5
        wobble = 0.014 * math.sin(t * 9.0) + 0.009 * math.sin(t * 21.0)
        step = 1.0 / rows
        tex.bar(
            "body",
            WHITE_FUR,
            (max(0.0, 0.5 + wobble - half_width), t - step * 0.5,
             min(1.0, 0.5 + wobble + half_width), t + step * 0.5),
        )
    tex.spots("body", WHITE_SHADE, count=26, seed=44, radius=2, alpha=70,
              sub=(0.43, 0.0, 0.57, 1.0))
    tex.spots("body", FUR_BLACK_WARM, count=18, seed=45, radius=2, alpha=55)
    tex.border("body", palette.hex_to_rgb(palette.GRIME), width=1, alpha=36)

    # Rear skull stays black. The face uses a frontal projection so the cap
    # triangles cannot repeat eyes or turn the chin patch into a forehead blaze.
    tex.fill("head", FUR_BLACK, jitter=6, seed=51)
    tex.fill("head", WHITE_FUR, sub=(0.33, 0.0, 0.67, 1.0), jitter=5, seed=54)
    tex.fill("muzzle", WHITE_FUR, jitter=5, seed=61)
    tex.fill("face", FUR_BLACK, jitter=5, seed=52)
    width, height = tex.cell("face")[2:]
    for row in range(height):
        y = 0.380 - (row + 0.5) / height * 0.184
        for col in range(width):
            x = ((col + 0.5) / width - 0.5) * 0.200
            color = None
            # White lower cheeks/throat, paired muzzle pads and narrow nose blaze.
            if (abs(x) < 0.063 and y < 0.253) or (
                (x / 0.049) ** 2 + ((y - 0.268) / 0.030) ** 2 < 1
            ) or (0.284 < y < 0.322 and abs(x) < 0.012 * (0.322-y)/0.038):
                color = WHITE_FUR
            # Broad black crescent directly below the white muzzle, with a white
            # throat below it; slightly fuller on the cat's left as photographed.
            if ((x - 0.003) / 0.038) ** 2 + ((y - 0.240) / 0.040) ** 2 < 1 and y < 0.252:
                color = FUR_BLACK
            for eye_x in (-0.046, 0.046):
                radius = ((x-eye_x)/0.017)**2 + ((y-0.308)/0.019)**2
                if radius < 1:
                    color = EYE_AMBER
                if radius < 0.55:
                    color = PUPIL
                if ((x-eye_x+0.004)/0.003)**2 + ((y-0.315)/0.003)**2 < 1:
                    color = WHITE_FUR
            # Pink triangular nose and short dark philtrum.
            if 0.267 < y < 0.282 and abs(x) < (y-0.267)*0.95:
                color = PINK_NOSE
            if 0.259 < y <= 0.269 and abs(x) < 0.0018:
                color = FUR_BLACK
            if color:
                tex.bar("face", color, (col/width, row/height, (col+1)/width, (row+1)/height))

    # Paws: white socks with toe separations.
    tex.fill("paw", WHITE_FUR, jitter=6, seed=71)
    tex.spots("paw", WHITE_SHADE, count=8, seed=72, radius=2, alpha=60)
    for x in (0.34, 0.66):
        tex.bar("paw", WHISKER, (x, 0.0, x + 0.028, 0.42))
    tex.bar("paw", (196, 192, 184), (0.05, 0.93, 0.95, 1.0))

    # Neck: black nape with a broad white throat, continuing the chest bib.
    tex.fill("neck", FUR_BLACK, jitter=6, seed=87)
    tex.streaks("neck", FUR_BLACK_WARM, count=5, seed=88, alpha=36)
    tex.fill("neck", WHITE_FUR, sub=(0.26, 0.0, 0.74, 1.0), jitter=6, seed=89)
    tex.spots("neck", WHITE_SHADE, count=6, seed=90, radius=2, alpha=60, sub=(0.26, 0.0, 0.74, 1.0))

    # Legs and tail: black with a faint sheen; ears: black with inner shading.
    for region in ("leg", "tail"):
        tex.fill(region, FUR_BLACK, jitter=6, seed=81 if region == "tail" else 85)
        tex.streaks(region, FUR_BLACK_WARM, count=6, seed=82 if region == "tail" else 86, alpha=36)
    # Only the right hind leg has a white band. Its inner white panel runs
    # upward into the belly; a black lower leg separates the band from the sock.
    tex.fill("ring_leg", FUR_BLACK, jitter=6, seed=94)
    for row in range(64):
        v = row / 63
        for col in range(64):
            u = col / 63
            band = 0.49 + 0.025 * math.sin(u * math.tau)
            if band < v < band + 0.14 or (v < band + 0.14 and 0.10 < u < 0.40):
                tex.bar("ring_leg", WHITE_FUR, (col/64, row/64, (col+1)/64, (row+1)/64))
    tex.fill("ear", FUR_BLACK, jitter=5, seed=91)
    tex.fill("ear", (66, 58, 60), sub=(0.0, 0.0, 0.5, 1.0), jitter=4, seed=92)
    tex.spots("ear", (98, 86, 86), count=6, seed=93, radius=2, alpha=55, sub=(0.0, 0.0, 0.5, 1.0))


def _add_ear(p: PropBuilder, tex: Texture, side: float) -> None:
    """One flattened triangular ear: front triangle, back triangle, three rims."""
    base_x = side * 0.052
    base_y = 0.330
    base_z = 0.243
    width = 0.070
    height = 0.072
    depth = 0.026
    lean = side * 0.018  # tips lean slightly outward

    outer = tex.uv("ear")
    inner = tex.sub("ear", 0.52, 0.06, 0.98, 0.94)
    left = (base_x - width * 0.5, base_y, base_z)
    right = (base_x + width * 0.5, base_y, base_z)
    apex = (base_x + lean, base_y + height, base_z - 0.006)

    def shifted(point, dz):
        return (point[0], point[1], point[2] + dz)

    front = (shifted(left, depth * 0.5), shifted(right, depth * 0.5), shifted(apex, depth * 0.5))
    back = (shifted(left, -depth * 0.5), shifted(right, -depth * 0.5), shifted(apex, -depth * 0.5))

    inner_uvs = [(inner[0], inner[3]), (inner[2], inner[3]), ((inner[0] + inner[2]) * 0.5, inner[1])]
    p.mesh.triangle(front[0], front[1], front[2], inner_uvs, COAT_TINT, shade_mult=0.95)
    outer_uvs = [(outer[2], outer[3]), (outer[0], outer[3]), ((outer[0] + outer[2]) * 0.5, outer[1])]
    p.mesh.triangle(back[0], back[1], back[2], outer_uvs, COAT_TINT, shade_mult=0.64)
    rim_uvs = [(outer[0], outer[3]), (outer[2], outer[3]), (outer[2], outer[1]), (outer[0], outer[1])]
    p.mesh.quad(front[0], back[0], back[2], front[2], rim_uvs, COAT_TINT, shade_mult=0.80)
    p.mesh.quad(back[1], front[1], front[2], back[2], rim_uvs, COAT_TINT, shade_mult=0.90)
    p.mesh.quad(back[0], back[1], front[1], front[0], rim_uvs, COAT_TINT, shade_mult=0.72)


def build_spooner_man(p: PropBuilder) -> None:
    tex = p.set_texture(256, seed=7)
    for name, rect in REGIONS.items():
        tex.region(name, rect)
    _paint_coat(tex)

    # --- torso: stocky, fuller over the hindquarters ------------------------
    p.lathe(
        base=(0.0, TORSO_CENTRE_Y, TORSO_REAR_Z),
        profile=[
            (0.000, 0.066),
            (0.055, 0.112),
            (0.145, 0.128),
            (0.255, 0.126),
            (0.360, 0.117),
            (0.440, 0.096),
            (0.490, 0.052),
        ],
        segments=10,
        axis="z",
        uv=tex.uv("body"),
        cap_uv=tex.sub("body", 0.02, 0.80, 0.06, 0.90),
        color=COAT_TINT,
        ellipse=(1.12, 0.92),
        rotation=math.pi / 2,
    )

    # --- neck: a rising, tapered shoulder-to-jaw transition ----------------
    # Start inside the shoulder and finish inside the skull. Ten aligned
    # segments match the neighbouring masses, avoiding the exposed octagonal
    # collar of the old horizontal neck. UVs keep the existing black nape and
    # white throat exactly as painted.
    neck_start = len(p.mesh.positions)
    neck_base_z = 0.040
    neck_length = 0.200
    neck_base_y = TORSO_CENTRE_Y
    neck_rise = 0.061
    neck_profile = [(0.000, 0.108), (0.055, 0.106), (0.110, 0.094),
                    (0.155, 0.080), (0.200, 0.068)]
    p.lathe(
        base=(0.0, neck_base_y, neck_base_z),
        profile=neck_profile,
        segments=10,
        axis="z",
        uv=tex.uv("neck"),
        color=COAT_TINT,
        ellipse=(1.05, 0.96),
        rotation=math.pi / 2,
        cap_start=False,
        cap_end=False,
    )
    for index in range(neck_start, len(p.mesh.positions)):
        x, y, z = p.mesh.positions[index]
        p.mesh.positions[index] = (x, y + neck_rise * (z - neck_base_z) / neck_length, z)

    face_index_start = len(p.mesh.indices)

    # --- head: broad and rounded, sitting forward of the shoulders ----------
    p.lathe(
        base=(0.0, 0.286, HEAD_BASE_Z),
        profile=[
            (0.000, 0.050),
            (0.045, 0.082),
            (0.105, 0.088),
            (0.165, 0.074),
            (0.200, 0.038),
        ],
        segments=10,
        axis="z",
        uv=tex.uv("head"),
        cap_uv=tex.sub("head", 0.02, 0.60, 0.06, 0.70),
        color=COAT_TINT,
        ellipse=(1.05, 0.94),
        rotation=math.pi / 2,
    )

    # --- muzzle: short projection with the white pad and pink nose ----------
    p.lathe(
        base=(0.0, 0.264, MUZZLE_BASE_Z),
        profile=[(0.0, 0.030), (0.022, 0.046), (0.050, 0.040), (0.062, 0.024)],
        segments=8,
        axis="z",
        uv=tex.uv("muzzle"),
        color=COAT_TINT,
        ellipse=(1.18, 0.82),
        rotation=math.pi / 2,
        cap_start=False,
    )

    # Project the front half of the skull and muzzle onto one continuous face.
    u0, v0, u1, v1 = tex.uv("face", inset=0.5)
    face_vertices = set()
    for offset in range(face_index_start, len(p.mesh.indices), 3):
        triangle = p.mesh.indices[offset:offset + 3]
        if min(p.mesh.positions[index][2] for index in triangle) >= HEAD_BASE_Z + 0.105 - 1e-6:
            face_vertices.update(triangle)
    for index in face_vertices:
        x, y, _ = p.mesh.positions[index]
        p.mesh.uvs[index] = (
            u0 + (x / 0.200 + 0.5) * (u1-u0),
            v0 + (0.380-y) / 0.184 * (v1-v0),
        )

    _add_ear(p, tex, -1.0)
    _add_ear(p, tex, 1.0)

    # --- legs: tapered, with a feline hock and haunch at the rear -----------
    for side in (-1.0, 1.0):
        front_start = len(p.mesh.positions)
        p.lathe(
            base=(side * FRONT_LEG_X, 0.030, PAW_FRONT_Z),
            profile=[(0.000, 0.026), (0.020, 0.038), (0.070, 0.030), (0.112, 0.040), (0.180, 0.052)],
            segments=6,
            axis="y",
            uv=tex.uv("leg"),
            color=COAT_TINT,
            rotation=math.pi / 2,
            cap_start=False,
            cap_end=False,
        )
        # The upper foreleg leans back/inward into the shoulder, while the
        # ankle stays over the forward-facing paw. Bury its open top ring in
        # the torso instead of leaving a disconnected rim below the throat.
        for index in range(front_start, len(p.mesh.positions)):
            x, y, z = p.mesh.positions[index]
            shoulder = max(0.0, (y - 0.142) / 0.068)
            p.mesh.positions[index] = (x - side * 0.012 * shoulder, y, z - 0.040 * shoulder)
        # Facing +Z, anatomical right is -X. Mirror the band's UVs as well
        # so its white inner panel still joins the belly on the inward side.
        rear_uv = tex.uv("ring_leg" if side < 0 else "leg", inset=0.5)
        if side < 0:
            u0, v0, u1, v1 = rear_uv
            rear_uv = (u1, v0, u0, v1)
        p.lathe(
            base=(side * REAR_LEG_X, 0.030, PAW_REAR_Z),
            profile=[(0.000, 0.028), (0.024, 0.042), (0.075, 0.038), (0.150, 0.070), (0.196, 0.058)],
            segments=6,
            axis="y",
            uv=rear_uv,
            color=COAT_TINT,
            rotation=math.pi / 2,
            cap_start=False,
            cap_end=False,
        )

    # --- paws: forward-facing toes with heels enclosing the ankle --------
    for side in (-1.0, 1.0):
        for leg_x, leg_z in ((FRONT_LEG_X, PAW_FRONT_Z), (REAR_LEG_X, PAW_REAR_Z)):
            p.lathe(
                base=(side * leg_x, 0.023, leg_z - 0.032),
                profile=[(0.0, 0.014), (0.018, 0.030), (0.058, 0.034), (0.088, 0.018)],
                segments=8,
                axis="z",
                uv=tex.uv("paw"),
                color=COAT_TINT,
                ellipse=(1.05, 0.62),
                rotation=math.pi / 2,
            )

    # --- tail: rearward with a gentle rise, as in the side reference ------
    p.tube_path(
        points=[
            (0.000, 0.258, -0.300),
            (0.012, 0.278, -0.395),
            (0.026, 0.301, -0.492),
            (0.032, 0.314, -0.566),
            (0.030, 0.319, -0.612),
            # A level final span preserves the previous rear extent, so origin
            # normalization leaves the body, face and floor contact in place.
            (0.030, 0.319, -0.651984),
        ],
        radii=[0.040, 0.037, 0.034, 0.029, 0.023, 0.014],
        segments=8,
        uv=tex.uv("tail"),
        color=COAT_TINT,
        cap_end=True,
    )

    p.add_note(
        "low-poly tuxedo cat: lathe torso/head/legs, swept-tube tail, "
        "tuxedo markings painted into one 256x256 texture"
    )

    # His tail reaches further back than his nose reaches forward, so centre the
    # bounding box on the origin as the pack requires while keeping the natural
    # build coordinates above.
    p.mesh.normalize_origin()


PROPS = {"spooner-man": build_spooner_man}
