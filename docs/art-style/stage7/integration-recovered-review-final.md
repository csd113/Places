# Recovered weather-map compatibility — final Stage 7 result

2026-10-09. Both recovered Blizzard/Snowfall cases pass exact final package build, required-current and decoding checks; the completed [Stage 7 integration](README.md) supplies the separate whole-inventory/native acceptance. Recovered sources come from original hash-verified serialized semantics, not unavailable original pre-prepare source JSON.

## Blizzard: one-ULP roof underbound was a real change

The first rebuild preserved 143,251 semantic bytes, 840,286 navigation bytes and 262 of 264 collision boxes. Two pre-ridge roof records lowered only maxY from 4.300000190734863 (`0x4089999a`) to 4.299999713897705 (`0x40899999`), removing positive-volume top slabs of 1/2,097,152 m height. Their removed volume was not covered by any other box. This was not harmless partition serialization or epsilon equivalence.

Reconstructing a span maximum from interior f32/FMA samples underbounded the actual owned ridge. A foot at 4.298999786376953 plus the existing STEP_EPS gives the lower top: the original blocks_body predicate is true while the first rebuild is false. The highest-support witness at(8.15,-28) also differs. This is a precise predicate/support counterexample, not a claimed naturally visited route. Unchanged navigation cannot by itself prove traversal equality.

Geometry 7 evaluates true owned span endpoints consistently through renderer/collision/loader without borrowing an adjacent roof. The final compatibility comparison retains exact collision/navigation and interactive semantics, without a collider exception or byte-assertion waiver. Final Blizzard package SHA is `ef1bbc55baf05757127cd6acb445eee2086221003993f9f2c2b528c5c96af70a` for this dated source/compiler epoch.

## Snowfall: explicit legacy defaults

Original collision and navigation are byte-identical after default recovery. Raw semantics still differ by exactly four formerly omitted SnowfallDef defaults: intensity 1, storm severity 0, visibility 5 m and fog colour [.6800000071525574,.7300000190734863,.7900000214576721]. Applying exactly those defaults makes JSON equal and is idempotent. Raw bytes are not described as equal, and normalization does not imply compiler repeat-build idempotence.

Final Snowfall package SHA is `2deffc4ab4414f68bbfb5eab0d0d336b29701e3674572de88ad0c49ffdc0e0c9`. The final outward bright-backdrop repair is separately accepted with unchanged collision/navigation and preserved weather/material/source bounds in [content findings](content-reference-audit.md).

The completed Stage 7 publication uses Geometry 7/solver 16; [current verification](../../VERIFICATION.md) names later source currency. These precise recovered-package comparisons do not themselves establish displayed weather, every moving-entity state, physical platform GPU behavior or whole-map native traversal. Those claims remain limited to the actual integration controls.
