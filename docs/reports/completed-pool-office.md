# Completed Pool and Office models

15 October 2026. This batch adopts **33 delivered models: 23 Pool, nine Office
and one snack vending machine**. Nineteen replace existing exports and fourteen
add catalog entries. All GLB bytes match the completed Library deliveries;
`assets/completed-models.json` now contains 136 accepted exports with measured
bounds. Full editable and reference deliveries remain separately in Library and
the ignored local import collection.

## Implementation

The Pool adds a perimeter planter, framed safety notice and two manufactured
three-riser entry modules. Six hidden player supports and fitted item colliders
follow the treads; basin, shelf, tub and water volumes retain their dimensions.
The ladder collider follows the delivered rails and rungs. Tile/panel modules
are catalog assets rather than extra display objects in the demo.
The dense washer basket now starts and resets on the dry east deck; it remains
pickable, dormant until activated, and physically sinks when dropped into water.

Office adds reception furniture, a noticeboard, a tall filing cabinet and the
snack machine beside drinks. Existing layout, navigation, AI and modular ramp
rails remain. Either machine's E interaction cycles the shared
`break_room_vending` circuit through steady, flickering and dark states. The two
basement fixture failures remain independent. `props[].switch_group`, fixed
`light` components and numeric `emission_overrides` are reusable authoring fields.

The three fixture families draw their delivered GLBs. Their emitter apertures
follow the recessed round lens, two separate wall strips and the Office's
downward diffuser. Metal and gaskets remain non-emissive. Geometry revision 14
and PLMP v8 retain prepared fixture/prop controller identities; older prop
records remain readable.

The snack export retains 16,240 triangles, 13 material groups, five shelves,
50 dimensional packets, 25 coils and thin BLEND glass. Only its exact accepted
hash enables the bounded 16,240-triangle importer profile; ordinary GLB imports
retain their 6,000-triangle ceiling. Shared decoded images and ordinary opaque
materials remain shared; only luminous uniforms require controller ownership.
Thirteen unique embedded PNGs also exist as standalone repository assets.

## Lighting limits

Full switches visible emissions and prepared illumination together. Switched
sources are excluded from the permanent room/zone baseline, including authored
off sources. Static retains each prop's authored illumination and matching
emission; runtime requests carry into Full. This does not rebake illumination
or reflection captures dynamically. Reflected images retain their authored
capture state. Cardinal fixture placements are verified; arbitrary wall yaw
retains the existing axis-quantized finite-source sampling limitation.

Four independent Office circuits overflowed the existing four-page budget.
The shared vending circuit uses three prepared groups and permits six pages at
the unchanged density and byte limit. No renderer resource cap or sampling
density was lowered or enlarged to fit the art.

## Validation

Formatting, configured debug/release Clippy, all-feature workspace tests, the
exact default workspace test command and 359 Python tests passed. Catalog,
texture, completed-model and generator checks passed. Explicit developer runs
validated Pool steps, nine Office collision stations, water settling/pickup/
swimming, both vending interactions, basement failures and flashlight response.
Matched Full captures show panel and nearby floor illumination extinguishing
together; Static captures retain their authored state.

The Office Full package uses five atlas pages. Bounded 300-frame Metal trials
at a 1920×1080 drawable measured median GPU time of 1.19 ms for Pool and 1.17 ms
for Office; peak process RSS was 294 MiB and 1,074 MiB respectively. These are
single local workload samples, not a general performance guarantee.

The deck reset pose and inward-facing records row require refreshed native
captures. Drinks appearance is provisional pending the separately requested
replacement export; its lighting behavior has passed against the current model.
Final source/package currency and publication checks follow the remaining
approved Winter/Outdoors adoption and revised drinks delivery.
