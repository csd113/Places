# Original cabinet artwork

These opaque RGB inputs were generated with the built-in `image_gen` tool for
the October 12, 2026 cabinet refinement. The source concept boards were inspected
for art direction; their pixels were never copied into these textures. Side
inputs are 887×1774 (1:2), and accepted gameplay inputs are 1448×1086 (4:3).
The originals below are the production sources. Incidental `generated_images/`
copies and the discarded portrait shooter draft are not runtime dependencies.

To reproduce the fitted PNGs and GLBs from the retained inputs, from repository
root:

```sh
python3 tools/props/author_retro_arcade.py --only cabinet_racing cabinet_shooter cabinet_action cabinet_fighting cabinet_scifi
python3 tools/props/build.py --only arcade:cabinet_racing arcade:cabinet_shooter arcade:cabinet_action arcade:cabinet_fighting arcade:cabinet_scifi
```

The authorer preserves the explicit atlas rectangles in
`tools/props/parts/retro_arcade.py`. It fits imagery, composes original title and
game HUD text, writes 1024-square masters and Lanczos 256-square natives, and
authors the sci-fi highlight pigment mask. The normal builder reads those PNGs;
neither runtime startup nor map loading paints textures. A new generated source
must satisfy the documented aspect and source limits before replacing a file.

## Side print prompt set

Each request used this shared prompt, followed by its subject paragraph:

> Use case: illustration-story. Asset type: original artwork printed on a
> late-1990s upright arcade cabinet side, to be fitted into a 1:2 UV region.
> Create a single flat, opaque portrait illustration in a 1:2 aspect ratio.
> Style: nostalgic 1997-2002 airbrushed commercial arcade poster art, illustrated
> like colorful screen-printed vinyl with hand-painted volume, bold readable
> silhouettes and limited large shapes. Deep dark navy occupies at least half
> of the picture, pink and violet accent highlights, soft saturated colors.
> Leave the upper 20% mostly dark for a title later. Keep hero imagery in the
> middle 60%. This is printed art, no glowing outline, no real neon tubes, no
> physical cabinet, no photographed environment, no text, no logos, no watermark,
> no border. Avoid fine noise, busy small details, futuristic cyberpunk styling,
> flat emoji icons, childish stick figures.

- `cabinet_racing_side.png`: Subject: a red 1990s sports coupe with large dark
  tires speeding diagonally toward the viewer along a violet coastal road, a
  sunset of muted amber and hot pink, distant dark seaside palms, sweeping pink
  racing stripes. The chunky red car must be the unmistakable hero, centered
  with room for print cropping.
- `cabinet_shooter_side.png`: Subject: a chunky blue-grey retro spacecraft in a
  dynamic upward diagonal bank, orange thruster trails, distant violet crescent
  planet and a small formation of enemy ships. Deep midnight blue space, cyan
  light on the ship, magenta cosmic mist. The one large ship must be
  unmistakable at tiny texture resolution.
- `cabinet_action_side.png`: Subject: a tough fictional 1990s street-adventure
  hero in mustard jacket and denim, half body with raised gloved fist, facing
  right; a deep violet nighttime city silhouette and broad warm magenta/orange
  lightning brushstrokes behind. Commercial arcade painted illustration,
  confident bold face and gesture, readable at tiny texture resolution.
- `cabinet_fighting_side.png`: Subject: two fictional arcade martial artists,
  one in red and one in dark cyan-blue, posed facing each other in strong
  full-body fighting stances against a violet and hot-pink painted energy burst.
  Bold muscular silhouettes, cloth belts and stylized illustrated faces,
  classic 1990s fighting-game cabinet print with simple background.
- `cabinet_scifi_side.png`: Subject: a classic silver 1980s-retro rocket and a
  helmeted explorer beside it, large lavender crescent planet and blue rings
  behind, restrained mint-cyan highlights and magenta shadow on deep navy.
  Vintage science-fiction arcade painted poster, chunky spacecraft and smooth
  graphic planet shapes; keep the scene dark and readable.

## Gameplay prompt set

Each request used this shared prompt, followed by its genre paragraph:

> Use case: historical-scene. Asset type: flat original gameplay screen texture
> for a late-1990s arcade game, full-bleed 4:3 composition, viewed straight-on.
> Create only the in-game graphics, NO physical CRT, NO cabinet, NO room, NO
> frame, NO vignette, NO scanlines. Reconstruct a convincing original 1997
> arcade game screenshot with chunky low resolution artwork, crisp square
> sprite pixels and bold readable silhouettes; this will be downsampled to
> only128x96 so prioritize larger recognizable shapes over fine details.
> Nostalgic colorful 16-bit/32-bit game graphics, deep midnight blue and violet
> shadows, bright hot-pink/red/cyan accents. Fill all edges. Leave top12% as
> clean dark background for deterministic HUD later. No text, no letters, no
> digits, no logos, no watermarks. Avoid flat minimal vector symbols, childish
> stick figures, photographic imagery, modern cyberpunk UI.

- `cabinet_racing_screen.png`: Game genre: classic pseudo-3D coastal racing.
  Rear third-person view of one large red sports coupe occupying lower center
  quarter of screen, driving up a curving asphalt road with chunky alternating
  red-white rumble strips; two small opponent cars ahead. Purple mountains,
  blue ocean to the right, roadside palms silhouetted, sunset amber-orange and
  magenta sky near horizon. The perspective road must converge slightly left
  of center at the horizon. Sprite/polygon graphics from1996, recognizable car
  tail lights, broad blue rear window, large dark tires. Bright readable road
  and car.
- `cabinet_shooter_screen.png`: Game genre: vertical scrolling space shooter.
  Large silver cyan player spacecraft at lower center, three formations of
  distinct red-orange and cyan alien ships across upper-middle screen, broad
  orange laser bolts, one large pink explosion. Dark violet planet surface
  curves through lower-left background and blue-black starfield above. Tiny
  stars sparse. Crisp illustrated sprite graphics from1994; player and enemies
  have visible wings/cockpits and weapon pods, strong small silhouettes.
- `cabinet_action_screen.png`: Game genre: 1997 side-scrolling urban brawler.
  A mustard-jacket hero with blue jeans at left center and a red-jacket
  opponent at right center, both clearly human detailed pixel sprites in
  fighting stances, not stick figures. Midnight city alley with chunky
  storefronts, pink and blue neon signs without text, steel shutters, fire
  escape silhouette, several windows, wet brick pavement across lower quarter.
  Layered parallax scenery in dark navy/violet. Pixel sprites show heads/hair,
  arms, jacket folds, hands and shoes; warm color highlights.
- `cabinet_fighting_screen.png`: Game genre: 1998 two-player martial-arts
  fighting arcade. One red-clothed and one blue-clothed fictional fighter,
  large detailed muscular pixel sprites, facing each other in strong active
  martial arts stances, each occupying one third of image height, feet near
  the lower20%. Ancient tournament courtyard in twilight, large purple mountain
  silhouette, stone lanterns and temple columns, stylized sunset magenta and
  amber bands. Detailed broad sprite shading with sharp square pixels, cloth
  belts and readable arm/leg silhouettes, horizontal arena ground.
- `cabinet_scifi_screen.png`: Game genre: 1996 first-person sci-fi arcade
  exploration shooter. Deep perspective metallic space-station corridor with
  strong converging geometric wall and ceiling panels, blue violet ambient
  shades, broad amber/red warning lamp panels, one cyan armored alien soldier
  in middle distance just right of center, a blocky silver/cyan player blaster
  visible bottom right. Floor panels recede toward open dark blast door. Early
  low-poly textured 3D game screenshot with hand painted64px material detail,
  crisp geometry and readable depth, no modern reflections, no reticle or HUD.

The initial shooter request returned a portrait draft. The accepted source was
an image edit using that draft as target, with this prompt:

> Use case: precise-object-edit. Target: this original arcade game screenshot.
> Recompose its in-game scene into a LANDSCAPE 4:3 gameplay screenshot, 1448
> pixels wide by1086 pixels high. Maintain the same period pixel-art aesthetic,
> spacecraft designs, deep navy/cyan/magenta palette and scene. Move the one
> large silver/cyan player craft to lower center, make it smaller enough to show
> fully within the lower quarter, bring the large orange enemy craft and its
> two escorts into the upper middle half, retain broad laser bolts and the pink
> explosion. Flatten the diagonal planet into lower-left corner background.
> All important craft should remain visible with a wider combat field and more
> space on both left and right. Keep top12% clean dark for later HUD. No text,
> no UI, no logos, no border, no physical CRT. LANDSCAPE 4:3, NOT portrait.
