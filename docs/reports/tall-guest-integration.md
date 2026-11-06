# Tall Guest integration review

Tall Guest is an optional authored actor with independent movement, physical
pose, blind evidence tracking and native directional audio ownership. No shipping
map is changed or rebaked. The implementation was developed in separate build,
refinement and polish passes, each exercised in the native game.

## Frozen art and separate budget decision

The original approved runtime export is retained exactly: GLB SHA256
`a2d5f54c79bc9842428b1948c04175101d69d60adb51111d5f389b51d266ad92`,
5,970 triangles, 5,138 split vertices, 30 joints, 32 nodes, two materials,
nine clips and one 1024-square PNG. The 63,276-triangle authoring master is excluded.
The exact-export completed-model exception preserves the continuous hood/throat,
narrow organic pink lining, ivory stalk, pressed palms and three legs. Ordinary
3,000-character/256-texture art defaults and 6,000-triangle/1024-edge engine
ceilings remain unchanged. The exception was reviewed using actual native
materials/deformation/contact and release costs, not merely engine limits.
The complete unique embedded texture families total 93.742 MiB of the existing
128 MiB pack ceiling. No automatic reduction or global budget increase is used.

The matched all-feature release fixture uses High textures, static lighting off,
1800×1400 physical drawable, actual swap interval 0, and no recording/dumps/frame
pacer. Native logs confirm Immediate presentation on Apple M2 Pro / Metal. Alternating 0/1/8-actor runs measured one actor at 0.618–0.620 ms median
CPU frame; eight fully visible actors at 11.11–11.58 ms CPU, 3.99–4.40 ms GPU and
83–87 FPS. Zero/one controls exceeded 120 FPS (464–568 FPS), showing an uncapped
path. Eight actors are CPU-bound. This is a tiny fixture on one Mac, not a full
level, cross-hardware promise or claimed FPS gain. A rejected earlier eight-actor
run cost about one second per frame; conservative full-pose AABB rejection of
distant door leaves fixed that defect while retaining exact triangle SAT nearby.

## Gameplay and pose contract

Base speed follows 1.1 times actual configured player walking speed, pursuit
1.2 times; defaults are 4.125/4.5 m/s against the player's 3.75. Native controls
also use 3.3/3.6 against 3.0. Actual travel retimes authored contacts and upper-body
sway; controller owns root translation once. Squeeze is provisionally 1.2 m/s.
Numbers other than the explicit shared hearing radii are integration tuning.

The monster is blind. Two distinct correlated body echoes are required for
lock. Returns keep their original sample, validate the receiving path and expire;
prediction is bounded to 0.6 s and confirmed evidence to 1.5 s. Passive sound
paths respect floors, ceilings, closed transmitting materials and bounded portal
search. Crouch movement remains audible within 1 m from player centre, ordinary
walking within 15 m. Actual maximum-variant WAV RMS and authored ambient gains
affect hearing; player output mute/master volume does not.

An actual audible thrown landing establishes a causal source. Valid delayed
active echoes may follow that known rolling/bouncing prop to final rest, then
inspect it with three clicks. Quiet drops use no more gain/range than crouch
walking. Silent throws/unrelated bodies do not expose private coordinates.
Original release identity survives collisions and checkpoint restore. Six
recorded contact descriptors replay through the production controller as one
incident; that CPU replay is distinct from actual solver/GPU/device evidence.

Door entry is timed from actual leading geometry, available distance and clip
speed. Entry, passage and full exit use the same posed geometry/blends/contacts
as rendering, including live door leaves, props and player/creature occupancy.
Blocked paths retain the last legal pose, reverse verified history or wait for
bounded recovery. Scale stays one. Explicit player crouch-only passages remain
unavailable regardless of numeric fit. Escape has two planted stamps, one agony,
a six-second reaction cooldown and bounded search. No crush damage is invented.
Native 1.20×2.15 m/.24 m crossings, reverse/thick/thin/paired thresholds and
centimetre boundary controls are fixture-specific; there is no 0.90 m fit claim.

Direct head overlay stays disabled. Authored distributed fitting has independent
native evidence, legal body turning and door/inspection clearance priority. The
final imported/skinned bank has 5,608 eligible crossing-free samples, including
45 isolated narrow candidates; the rejected wide control produces 98 crossings.
Quaternion order error is at most 1.2e-7. Live 229-frame door and occupied-door
reviews also find no sampled head/hand/stalk crossings and exact agreement of
all 32 gameplay/render joint matrices. Samples do not prove arbitrary continuous
fit or enable the isolated direct candidate in production.

Polish adds full physical-pose persistence and private post-restore clearance,
strict semantic/identity validation, lifecycle disable/occupied reenable/despawn,
waveform-cursor recovery, epoch/sequence/generation fencing, actual replacement
fades and voice tails. Save after an authored despawn still follows the existing
fixed-membership refusal policy. Terrain uses actual ramp/tread surfaces and
bounded leg reach; sampled flat/turn/stair/door stance drift is below 2 micrometres,
penetration below 0.19 mm and stance float about 1 mm. The proposed 20 mm slip /
5 mm penetration limits remain provisional, not universal asset tolerances.

## Audio and validation scope

Only 15 original runtime stems are shipped. Eight reference/optional cues stay
isolated in the original pack and never layer with stems. Actual native output
exercised all 23 originals in 186 isolated speaker/HRTF/front/rear/closed/muted
trials (472 s), including two 12.3 s loop repetitions, natural retirement,
zero mute PCM and bounded measured peaks. Callback allocation/free counters and
settled/unloaded voice/storage checks pass. Loop checks found no silent interior
100 ms windows and bounded adjacent-sample steps. Head voice follows the final
acoustic-origin axis; contacts emit at the appropriate foot, and the 3.8 s agony
voice survives the shorter visual action. Native fault injection checks delayed,
duplicated and lost cosmetic delivery without stacked pursuit or voice revival.

The complete production bank is 47 cues / 85 WAVs, 66,221,488 decoded bytes
(63.15 MiB), leaving 887,376 bytes below the unchanged 64 MiB limit. Future sounds
must fit that limit. Internal device PCM is technical evidence; it does not
establish physical headphone localization, acoustic SPL, subjective listening
quality or emotional horror. Actual player-eye VFR doorway/refuge recordings
preserve native PCM and no debug overlays; no independent human comprehension
study was performed. The two corresponding subjective criterion portions remain
explicitly unrun, rather than being reported passed by waveform measurements.

The [53-criterion record](tall-guest-acceptance.md) contains 49 scoped technical
passes, two provisional sampled-contact passes and two partial subjective portions
that remain unrun. Full source-specific evidence is preserved outside build targets, with technical passes, provisional sampled tolerances, superseded
first-heard-point behaviour and perceptual limitations separated. Native process
exit alone is never behavioral acceptance. Historical failed/inconclusive fixtures
remain in evidence. Full Rust/Python/asset/package checks and exact publication
SHA/CI status are recorded in the final handoff; this document is not a substitute
for that publication receipt.

## Authoring and compatibility

See the Tall Guest section in `docs/MAP_AUTHORING_GUIDE.md`. The actor has one
motion/pose owner; incompatible physics, routes, animation and door ownership
are rejected rather than silently falling back to a static prop. Missing runtime
model/import/render ownership is a load failure. New compiled packages declaring
the actor require `tall-guest-v1`; older readers reject the unknown requirement,
and the current reader requires the capability for the source field. Existing
maps/saves without the creature retain their compatibility rules.

The explicit `places-tall-guest-pose-check` and `places-audio-audit` tools document
regression value and bounded costs in their READMEs. Native campaigns are outside
ordinary Rust tests/CI and never depend on shipping-map baking. BENCH-only fault,
capture and real-clock 30/60/120 pacing controls are unset for performance and
normal gameplay. The pacer never supplies a fake simulation delta.
