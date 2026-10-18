# Lightmap tile size analysis

Existing lightmaps offer a large raw-record reduction but a small compressed-download reduction. **Keep the existing PLC1 codec for this stage.** No archive, source asset or Rust file was changed, and no Cargo command or bake ran. All sizes below use decimal MB; paired DEFLATE results are an in-memory framing experiment, not a shipped package measurement.

## Dataset and framing

The eight retained environment archives contain legacy `PLLC` blobs, which the current `PLC1` decoder does not accept directly. `git show af94e3f1 -- src/engine/package/compact_lightmaps.rs` confirms that the module changed only its magic from PLLC to PLC1; revision-one framing and coefficient semantics stayed the same. Legacy measurements are explicitly separate from current-format evidence.

Both use a 24-byte header: magic, u16 version, u16 tile edge, and four u32 fields (page edge, pages per group, group count, reserved). Each 16×16 tile is either compact (4-byte mode +48-byte ranges +256×8-byte words) or exact (4-byte mode +256×12-byte six-half texels). All framing and end-of-record lengths were checked.

| Legacy map | Pages × groups | Compact / exact tiles | Constant / positive-zero tiles | Identical repeats / nonconstant repeats | Raw MB | Actual ZIP-entry MB | Paired ZIP saving MB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| office_demo | 5 × 4 | 64,636 / 17,284 | 50,957 / 49,505 | 50,950 / 1 | 188.90 | 37.31 | 0.44 |
| retro_arcade_demo | 8 × 3 | 46,393 / 51,911 | 18,095 / 14,932 | 18,100 / 14 | 257.10 | 85.27 | 0.19 |
| lantern_hollow | 7 × 1 | 9,781 / 18,891 | 3,884 / 3,826 | 3,883 / 7 | 78.65 | 38.53 | 0.05 |
| winter | 5 × 1 | 4,418 / 16,062 | 676 / 428 | 668 / 2 | 58.68 | 29.56 | 0.02 |
| beach_demo | 7 × 1 | 21,941 / 6,731 | 13,429 / 776 | 13,461 / 58 | 66.78 | 17.12 | 0.14 |
| frutiger_aero_demo | 4 × 1 | 11,297 / 5,087 | 7,281 / 2,929 | 7,447 / 178 | 39.37 | 13.00 | 0.08 |
| pool_showcase | 2 × 1 | 4,135 / 4,057 | 3,223 / 3,223 | 3,222 / 0 | 21.16 | 5.96 | 0.04 |
| home_showcase | 1 × 1 | 2,227 / 1,869 | 1,915 / 1,915 | 1,914 / 0 | 10.43 | 2.34 | 0.03 |

Across 286,720 tiles, 99,460 are constant and 77,534 are strict positive zero. Only 260 repeated tiles are nonconstant. Total lightmap records are **721.08 MB raw / 229.09 MB in the existing ZIPs**.

## Lossless opportunity and actual compression limits

An analysis-only grammar replaces all-zero tiles with four bytes, constant compact tiles with their unchanged 48-byte ranges and one unchanged 8-byte word (60 bytes total), constant exact tiles with their unchanged six halves (16 bytes total), and eligible identical tiles with an eight-byte backward reference. Every candidate reconstructed its original encoded tile bytes exactly, so existing decoded half coefficients and compact-quantization decisions are unchanged. No prototype payload was written or accepted by the engine.

Constant-only framing removes **207.24 MB raw**, but paired raw-DEFLATE level-6 compression saves only **963,247 bytes**. Adding references removes another 1.69 MB raw and only **19,533 compressed bytes**. Combined: **208.93 MB raw versus 982,780 compressed bytes** (~0.43% of the actual legacy lightmap ZIP bytes). Repeated zero/constant runs already compress very well.

The paired baseline uses Python zlib DEFLATE level 6, while shipped ZIP entries use the Rust backend. Their baseline byte counts differ; the experiment isolates framing changes using one compressor and does **not** establish final collection download savings. A six-half constant mode could reduce the conservative compact-constant seed further, but reference sharing leaves only a few unique constant seeds; it does not change the conclusion.

## Current PLC1 confirmation

Current Relay Annex records were resolved through project.json aliases and have compiler fingerprint `b6d3aecaf75e…`. The existing Full Hub inspection uses compiler `96fce18b9910…`; it is a current PLC1 format sample, not the final collection source or final-compiler acceptance.

| Current PLC1 record | Pages × groups | Compact / exact | Constant / zero | Raw / actual ZIP MB | Paired ZIP saving bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| relay_intake | 1 × 1 | 3,386 / 710 | 2,708 / 2,592 | 9.29 / 1.82 | 35,706 |
| relay_tower | 2 × 2 | 11,340 / 5,044 | 6,450 / 6,366 | 39.33 / 14.08 | 83,658 |
| relay_discharge | 1 × 1 | 3,339 / 757 | 2,495 / 2,495 | 9.34 / 2.29 | 35,230 |
| hub | 1 × 1 | 1,839 / 2,257 | 782 / 782 | 10.80 / 4.36 | 12,618 |

Current Relay lightmaps save only 154,594 paired compressed bytes in total; Hub saves 12,618 bytes. This corroborates the legacy result. The separately measured exact prop-stream work offers much larger download savings and should take priority.

## Memory and preserved behavior

The current loader expands PLC1 to two RGBA16F planes per page per independent group, retained in shared CPU backing; the GPU uses the same 16 bytes per texel pair. Tile encoding alone does not reduce this residency. Old Office requires 335.54 MB expanded atlas data and Arcade 402.65 MB. Their raw-input plus expanded-output decode overlap is at least 524.45 MB and 659.76 MB, respectively, before metadata, other assets and staging. These are format-derived lower bounds, not measured game process peaks.

Office has two wholly positive-zero page pairs (groups 1 and 2, page 3), accounting for 33.55 MB expanded. Avoiding their allocation would require a separate runtime/page-indirection design; the present disk-codec experiment does not achieve it. The eight archives total 1,174.41 MB expanded across all maps, not simultaneous selected-map residency. The analysis process itself peaked at 607.17 MB RSS.

Keep group/page counts and ordering, independent switch contributions, texture and lighting settings, decoded precision, lightmap density and all authored visuals. Do not discard switch groups or lower atlas quality to claim a lossless storage win. This analysis recommends no lightmap engine changes or rebakes.

Evidence: `local/environment-collection-0.12.0/size-analysis/lightmap_tiles.json` (legacy counts, per-page/group breakdown, prototypes and exact-byte checks); `lightmap-current-tiles.json` (separate current PLC1 data); `lightmap_tiles.py` (read-only reproduction).
