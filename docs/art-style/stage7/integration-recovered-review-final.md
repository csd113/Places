# Final recovered weather-map compatibility audit

[Exact machine witness](integration-recovered-review-final.json). both exact recovered final cases passed.

The authoritative per-case campaign is `target/verification/map-regression-20261009T042109Z-30226`. Final archives are read only after their exact build / require-current / validate receipt passes. The frozen normal compiler SHA is `f3f27db928af9927219f3440a95acbbcfb2f53bdecb321af207613ddce91527f`, geometry revision 7 and solver revision 16; recorded source-file hashes and the installed/preserved compiler hashes are checked.

**Whole campaign and native acceptance remain pending.** Per-case equality below does not establish final renderer frames, displayed weather, moving-entity coherence, native traversal or compiler C2 reuse.

The earlier [failed Blizzard comparison](integration-recovered-review-audit.md) and its byte-verified C1 archives/command receipts remain unchanged and linked. The two genuine positive-volume roof slabs and body/support counterexample are retained; no epsilon equivalence, collider exception or raw-byte waiver is used.

All selected manifest role sizes/SHA hashes are validated. The decoder checks every PLCL2 field and metadata section, every navigation cell/mask/region run, finite/bounded values and trailing bytes. Every compiled quality variant shares the same collision/navigation entries. Exact occupied and step-class unions are checked by subtracting every opposing closed box in both directions; all primitives have positive volume. Every exact decoded collision field is included in JSON.

## blizzard_review

passed per-case receipt; exact final gameplay compatibility audited.

Recovered source SHA `d8c0196ec161e94c5116ee50ec81e670571788f1af1e0a43666618120955422a` is byte-identical to the preserved original semantics. Original package SHA `48733fd5930b02bb85c147a04710d307527a637f2d615c7eeaeb2ad178bcf7e6`; superseded C1 SHA `48af46a8afdf190c674813e80e4043517d9068be5e6b15340779aff7c39c9c4e`.

Final package SHA `ef1bbc55baf05757127cd6acb445eee2086221003993f9f2c2b528c5c96af70a`. Its declared compiler-inputs record pins the unchanged source, captured-reflection mode and frozen compiler SHA. Its passed receipt and exact manifest/header metadata are saved in JSON.

Collision raw bytes equal original: **True**. Navigation raw bytes equal: **True**. All decoded collision fields equal: **True**. Full occupied union equal: **True**. Step-class unions and all non-weather/interactive semantics are checked separately.

The exact collision section lengths, counts, raw hashes and all fields are retained in JSON, including floor/ramp/stair/region, roof profiles, water and ladders.

Blizzard semantics, collision and navigation require exact original bytes. The retained records 28/30 body/support witness is recomputed for original, superseded C1 and final. It is recorded as saved scalar predicate evidence, with native reachability/traversal outside this audit.

## snowfall_contrast

passed per-case receipt; exact final gameplay compatibility audited.

Recovered source SHA `dedfe1e664e32767b9084ccd303f083069ea1b4d3d030ebc77b663580d84ad11` is byte-identical to the preserved original semantics. Original package SHA `447f57fc993eef484e7c5a0f2142132c4085b02ebaa480f00b1ffa194f051499`; superseded C1 SHA `66ea73c409ca914ec9995ec5f21a0b39ed969e90aa1de61429f1c78f16f57755`.

Final package SHA `2deffc4ab4414f68bbfb5eab0d0d336b29701e3674572de88ad0c49ffdc0e0c9`. Its declared compiler-inputs record pins the unchanged source, captured-reflection mode and frozen compiler SHA. Its passed receipt and exact manifest/header metadata are saved in JSON.

Collision raw bytes equal original: **True**. Navigation raw bytes equal: **True**. All decoded collision fields equal: **True**. Full occupied union equal: **True**. Step-class unions and all non-weather/interactive semantics are checked separately.

The exact collision section lengths, counts, raw hashes and all fields are retained in JSON, including floor/ramp/stair/region, roof profiles, water and ladders.

Snowfall raw semantics remain unequal. The complete JSON difference is four added `SnowfallDef` defaults from `src/weather.rs`: intensity 1, storm severity 0, visibility 5 m, and fog colour `[0.6800000071525574,0.7300000190734863,0.7900000214576721]`. Applying exactly those defaults yields equal JSON and is idempotent. No raw byte assertion is waived, and this does not infer compiler repeat-build idempotence.

No source, asset, canonical contract or test edits were made. No Cargo, compiler, bake, native, target-writing or Git operation was run by this specialist. This report is limited to the recovered weather-map compatibility allocation.
