# How to play

Follow the missions in the upper left: they walk you from your first piece of wood to a working camp.

## Controls

| Key | Action |
| --- | --- |
| W A S D | Move |
| Shift | Sprint (stamina) |
| Space | Jump; hold in the air to glide (needs a glider) |
| C | Crouch (wild creatures notice you less) |
| Ctrl | Roll |
| Mouse | Look |
| Left mouse | Attack, swing a tool; place a building in build mode |
| Right mouse | Aim the bow; cancel build mode |
| R | Reload the bow; hold R and scroll to pick an arrow type; rotate a building in build mode |
| Mouse wheel | Switch weapon slot |
| F | Pick up, open, talk to a camp creature; hold F to build, craft or work at a station. With nothing in reach: the companion's bond skill |
| Q | Hold to aim an orb (shows the capture chance), release to throw |
| 2 | Next orb type (Capture, Fine, Grand) |
| 1 / 3 | Previous / next party slot |
| E | Summon or recall the selected creature |
| 4 | Companion stance |
| Middle mouse | Send the companion at what you aim at |
| Tab | Inventory, party, status points |
| T | Technology |
| B | Build menu |
| X | Dismantle mode (hold left mouse on a building) |
| F5 / F9 | Quick save / load |
| Esc | Close menus; settings (fullscreen, sound, quit) |
| F1 | Inspector (debug readout) |
| F2 | Free crafting (ignore missing materials) |
| F3 | Mute |

## The loop

1. **Gather.** Hit trees for wood and fibre, rocks for stone, blue crystals for Spark Shards. Pick up branches,
   pebbles and berries with F. An axe chops faster, a pickaxe mines faster.
2. **Build and craft.** Open the build menu (B), place a Workbench and hold F on the blueprint. At the workbench
   (F) pick a recipe, set the amount and start; hold F to help.
3. **Level up.** Everything gives experience. Each level gives technology points (T) and a status point (Tab).
4. **Capture.** Weaken a wild creature, then hold Q and throw an orb. Lower HP, a stronger orb, a burning or
   poisoned target, or one that hasn't noticed you all raise the chance.
5. **Found a camp.** Build a Camp Core. Press F on it to move creatures from your party or its storage to the camp.
   Camp creatures pick work by their job talents: Kindlet lights the campfire and smelter, Sprigling plants and
   waters, Tinkit crafts and hauls, Mossnut mines and chops, Fluffin ranches.
6. **Keep them happy.** Fill a Feed Box with food, build nests for the night, and use the Job Board to set job
   priorities (1 = first, 5 = last, Off = never) or assign a creature to one building.

## The creatures

| Creature | Element | Jobs | Bond skill |
| --- | --- | --- | --- |
| Fluffin | Neutral | ranch, gather, haul | Fluff Gift |
| Mossnut | Earth | plant, gather, mine, chop | Shell Guard: a shield on your arm |
| Sprigling | Leaf | plant, water, craft, gather, brew | Seed Shot |
| Kindlet | Fire | kindle | Flame Breath |
| Tinkit | Spark | craft, haul, mine, power | Pack Pouch: more carry weight |

Fire beats Leaf and Frost, Water beats Fire and Earth, Leaf beats Water and Earth, Spark beats Water and Frost,
Earth beats Spark and Fire, Frost beats Leaf and Earth (`content/rules.ron`, `element_chart`).
