# Sound effect prompts

Generated 2026-10-29 with ElevenLabs Sound Effects v2 (`eleven_text_to_sound_v2`, prompt influence 0.3, one take each, about 50 credits per clip). The raw takes are in `raw/`; `python -I tools/sfx/process.py` turns them into `assets/sfx/*.ogg`, every clip at the same loudness (the volumes in `sfx.rs` set the mix). A clip in `hand/` (the user's own edit) is copied unchanged: `hand/ui_open.ogg` is the user's shorter, quieter cut of the leather bag.

| Clip | Prompt | Used for |
|------|--------|----------|
| ui_click | Soft short UI button click, clean plastic tick, close-mic | buttons, weapon switch; low-pitched for rejections |
| ui_open | Opening a soft leather satchel, low muffled flap and gentle rustle of leather, no squeak, close-mic | menu open (lower pitch on close); replaced the first take ("Quick airy menu panel swoosh opening, light and bright"), which was a squeaky high pitch |
| swing | Fast melee weapon swing whoosh through air, short | player attack |
| hit | Blunt wooden club hitting a soft furry creature, punchy thud impact | hits on creatures |
| creature_cry | Small cute fluffy sheep creature bleat, high-pitched cartoon baa, single short cry | creature hurt / knocked out |
| pickaxe | Iron pickaxe striking rock, sharp metallic clink with stone chips | hits on rock and ore |
| axe | Axe chopping into a tree trunk, solid wooden thock | hits on trees |
| node_break | Boulder cracking and crumbling apart, rocks tumbling, short | tree/rock destroyed |
| pickup | Cheerful item pickup pop, soft bubbly blip, game collectible | items into the bag, harvest |
| throw | Small glass ball thrown hard, spinning whoosh with a faint whistle | Capture Orb throw |
| shake | Metal capsule wobbling on the ground, quick rattle and click | capture shakes |
| capture | Magical capture success chime, bright sparkling bell arpeggio rising | capture success |
| breakfree | Energy capsule bursting open, sharp electric pop and glassy crack | broke free / capture failed |
| build_place | Wooden planks dropped into place, hollow wooden clatter and thud | blueprint placed |
| build_done | Two hammer knocks on wood followed by a soft completion ding | construction finished |
| craft_done | Crafting complete notification, warm two-note marimba ding | station output ready, crop ready |
| levelup | Short triumphant level up fanfare, bright brass and sparkle, game jingle | player / camp / creature level up |
| step | Single footstep on soft grass and dirt, close-mic | footsteps |
| glide | Canvas glider snapping open in the wind, cloth flap and rush of air | glider opens |
| splash | Person jumping into a lake, medium water splash | entering water |
| summon | Creature released from a capsule, soft magical whoosh with shimmering sparkle | summon (higher pitch on recall) |
| eat | Crunchy bite and chewing of a fresh berry, short munch | player / creature eats |
| cry_mossnut | Cute small tortoise creature grunt, low gentle hollow croak with a soft woody shell knock, single short cry, cartoon game creature | Mossnut hurt / defeated |
| cry_kindlet | Cute little fire salamander creature chirp, quick raspy squeak with a tiny crackle of flame, single short cry, cartoon game creature | Kindlet hurt / defeated |
| cry_fluffin | Cute fluffy bunny creature squeak, soft high-pitched airy "pip" with a little sniff, single short cry, cartoon game creature | Fluffin hurt / defeated |
| cry_tinkit | Small raccoon creature chatter, quick mischievous chittering trill with a faint electric zap, single short cry, cartoon game creature | Tinkit hurt / defeated |
| cry_sprigling | Tiny leafy plant sprite creature coo, soft breathy bubbly "pweep" with a rustle of leaves, single short cry, cartoon game creature | Sprigling hurt / defeated |
