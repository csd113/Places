# Dense capacity: measured lossless storage — 2026-10-09

Stage 7's normal combined dense archive now fits the unchanged 1 GiB aggregate guard. PLMP v6 preserves every ordered triangle corner and all authored content; no geometry, source, quality fallback or allocation guard is dropped. Final [inventory acceptance](README.md) and [costs](performance.md) distinguish this completed result from the earlier rejected combined archive.

## Actual original and accepted records

The unchanged source has 5312 props, 48 models, 2760 solid props, 120 fixtures and 18 routes. Its original combined decoded package required 1,435,049,755 bytes and was rejected before partial publication. No preserved historic dense Full atlas was found; a report of old three-variant compilation is not evidence that its Full illumination existed.

| Quantity | Original | PLMP v6 |
| --- | ---: | ---: |
| Prop record per quality |473,890,078 B|340,854,707 B|
| Stored vertex slots per quality |6,665,147|6,297,672|
| Ordered indices per quality |6,923,034|6,923,034|
| Combined uncompressed package |1,435,049,755 B, rejected|1,035,943,642 B, accepted|
| Physical accepted archive |Unavailable|334,585,036 B|

Off/Medium/Full separately compiled original references decoded to 480,622,719/480,580,726/480,494,474 B. Off has no atlas as intended; both lit references explicitly retain page-overflow vertex-lighting fallback. Their separate current/decoder passes did not satisfy the normal combined archive gate or establish lost historical Full availability.

## Exact representation and safety

Whole-record content addressing already shares identical blobs; ZIP compression cannot reduce the decompressed guard. A complete vertex carries 69 bytes of position, colour, material UV, normal, tangent/handedness and atlas address/page. Sharing only exact full vertices within each batch removes 367,475 redundant slots per variant, preserving every ordered corner; this alone remains over the aggregate limit.

The measured batches contain 2,001,957 distinct exact 28-byte normal/tangent/handedness frames, with maximum 22,374 frames in one batch. The optional palette stores those bytes without rounding or normalizing; vertices retain their 36-byte prefix, five atlas bytes and u16/u32 frame reference. The writer chooses literal or palette form only when strictly smaller. Batch/material/caster/bounds data and u16 ordered indices remain exact. PLMP 3/4/5 readers retain their documented defaults.

Independent Off/Medium/Full comparisons check every ordered 69-byte triangle corner, metadata, predicted frame/reference wire byte and canonical re-encoding. All pass. No source/catalogue/dependency or non-prop record differs. Signed-zero, seam-attribute, malformed mode/count/reference/truncation/nonfinite and expanded-budget controls retain strict rejection. Encoded and expanded prop limits remain 512 MiB; the dense record expands to 448,534,303 B in literal layout, and the aggregate keeps 37,798,182 B headroom.

Actual combined building takes 303.866 s, or 313.740 s including required-current/decode. Original separate references take 315.231 s including validation, a different workflow; this does not establish paired bake speedup. Two-second samples observe 2,448,932,864 B compiler RSS, not exact peak/native/GPU residency. Native/RSS/transition limitations remain in [performance](performance.md).
