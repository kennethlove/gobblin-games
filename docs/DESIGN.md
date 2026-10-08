# Gobblin Games design brief

Gobblin Games is a multi-team, multi-goblin battle royale simulator. The game is
played entirely in the browser and via text.

| Usage | Font |
| ----- | ---- |
| Body | Atkinson Hyperlegible Next |
| Headlines | Grenze Gotisch |
| Logo | [JSL Blackletter](https://www.dafont.com/jsl-blackletter.font) |
| UI | Atkinson Hyperlegible Mono |

## Design goals

The design should have whimsy where possible. Discworld is the main theme
influence but the game is not visibly inspired by Discworld. The goblins are not
monsters, just a species that exists. They're humanoid but smaller and not scary
ugly like Discworld goblins are. They're a playful species and these games are
really just for fun, even though death is involved.

Delight is important.

Utility pages like home, FAQ, etc can have more fun than the in-game pages. Game
day detail pages and goblin detail pages will have a lot of information on them
so they'll need constrained fun.

Reasonably readable on all screen sizes. As much semantic HTML and CSS as
possible. Minimal JavaScript.

## Game details

The game offers a lot of customization so hard and fast rules like "8 teams" are
difficult to declare. Players can create custom goblins or have them
auto-generated; same for games, they're customizable but also have reasonable
defaults.

During a game, goblins can run, hide, fight, hunt, shelter, and more. They have
to react to events both in the arena and from other goblins. Goblins can start
with or acquire phobias, addictions, and injuries. They can form and break
alliances. Goblins can acquire weapons, shields, illicit drugs, and more.

A game continues, each round lasting 2 hours, until only one team survives. The
dead goblins are then resurrected by a friendly necromancer. They'll do it all
again next time.
