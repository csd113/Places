# It Only Exists in the Beam

One Explore entry contains four chapters: **Home Ã¢â€ â€™ Pool Ã¢â€ â€™ Office Ã¢â€ â€™ Arcade**.

Press **E** to pick up or put down an item. Leave each item on its return
furniture; picking it up or moving it away undoes the return. All three items
must remain in place to unlock the next door. **F1** tells you where to find the
next missing item and where to put it.

Press **F** to switch your flashlight on or off. The creature has no visible body. Its shadow appears on floors, walls and
furniture only in your flashlight beam. Keep the beam on it to stop it. It can move again when you look away, turn off the light, or let a
wall or piece of furniture block the beam. The entrance gives you a safe place
to put things down and read the hints.

| Chapter | Find | Put it on |
| --- | --- | --- |
| Home | Kitchen mug, upstairs books, sitting-room cushion | TV cabinet by the entrance |
| Pool | Rubber duck, blue towel, detergent | Plastic service table, service bench, empty rack's top shelf |
| Office | Archive records, mug, supply box | Filing-room cabinet, break-room table, reception counter |
| Arcade | Plush cat, star-patterned gift, blue bowling ball | Round prize table, prize counter, empty foyer ball rack |

Hold **C** to crouch when picking up the detergent from its low shelf. After returning
all three Arcade items, leave the foyer and keep the moving shadow in your beam for
two uninterrupted seconds from less than eight metres away. Then use the exit.

The forward doors advance to the next chapter. Back doors restart the previous
chapter, including its deliveries. The interactive EXIT sign restarts the current
chapter. After being caught, Enter restarts that chapter. After the final win,
Enter plays Arcade again. Choosing the game from the main menu starts at Home.
There is no inventory transfer or saved chapter checkpoint.

## Editing

Open this folder in Places Editor. Stable IDs are `beam`, `beam_pool`,
`beam_office` and `beam_arcade`; `beam` remains the entry. `menu_entry_only: true`
keeps the internal chapters out of Explore. Use Project Export or Build+Play for
the complete campaign. A selected-member build is not a standalone distribution.
Both Static and Full lightmaps are included.

The maintained two-storey Home has a furnished kitchen/dining/utility wing and
an upstairs bedroom. Pool combines changing rooms, a wet deck, a sunken basin
and a sloped bank with service rooms. Wide service openings and a rear landing
leave room for carrying; the towel bench runs lengthwise beside the table.
Office has workstations around a meeting
room, plus a filing room, break room and reception. Arcade has machine aisles,
party tables, a bowling alcove and a prize foyer.

The scene was expanded through the real editor's Project, Author, placement,
transform and Scripts controls. `tools/authoring/beam_editor.py` is a historical
recipe for an earlier single-map layout. These saved project sources are the
current authority; do not regenerate them from that recipe.

The scripts use existing navigation, physical-item receivers and native light
response. Patrol, last-seen/search pacing and catch rules are map-local logic
for the queued shared AI migration. Keep the objective furniture, refuge rules,
physical removal behavior and flashlight occlusion/hold behavior intact.

The visitor uses the existing per-entity `shadow_only` rendering option. The
posed model participates in the handheld flashlight shadow pass, while its
colour and reflection geometry stays hidden. Ceiling and baked light do not
reveal it. The same flashlight observation controls holding and release;
obstacles, range and the beam cone still apply.

Pursuit starts at 0.728 / 0.8064 / 0.896 / 0.9856 metres per second by chapter,
increasing by 0.0672 per returned item. These are 12% above the prior authored
speeds. Faint attached spatial footsteps use actual distance travelled, with
no footstep while the visitor is held or stationary. They use existing audio
assets and the normal effects/master-volume and unload lifecycle.
