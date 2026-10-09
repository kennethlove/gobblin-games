# game/src/

## Responsibility
Core game engine implementing the Gobblin' Games simulation. This directory provides pure Rust business logic with no I/O dependencies — all game state management, turn-based cycle execution, character lifecycle, area event handling, and message generation. The engine is deterministic and stateless except for message accumulation, making it suitable for both single-run simulations and stateful API integration.

## Design Patterns

### **Event Sourcing (Partial)**
- `messages.rs` implements a global message queue (`GLOBAL_MESSAGES`) that captures all game events chronologically
- Messages tagged by source (`MessageSource` enum: Game/Area/Character) enable event replay and audit trails
- `events/` module provides typed `GameEvent` enum as a structured, serde-friendly counterpart to stringly-typed `GameOutput`

### **State Machine**
- `Game` struct manages game lifecycle through `GameStatus` enum transitions: `NotStarted -> InProgress -> Finished`
- `Character` statuses follow deterministic state transitions: `Healthy -> RecentlyDead -> Dead` (with `Mauled` variant for animal kills)
- Turn phases enforce sequential execution: prepare → announce → execute → cleanup

### **Strategy Pattern**
- `Action` enum (in `characters/actions.rs`) encapsulates different character behaviors
- `ActionSuggestion` allows external influence on AI decisions (e.g., day-1 random move bias)
- `Brain` module uses different decision strategies based on context (enemy count, health, afflictions)

### **Template Method**
- `Game::run_day_night_cycle()` defines the skeletal algorithm:
  1. Check for winner
  2. Prepare cycle (`prepare_cycle`)
  3. Announce start (`announce_cycle_start`)
  4. Execute cycle (`do_a_cycle`)
  5. Announce end (`announce_cycle_end`)
  6. Clean up deaths (`clean_up_recent_deaths`)
- Subphases (area events, character processing) vary while overall flow remains constant

### **Builder Pattern (Implicit)**
- `Game::default()` uses `WPGen` to generate random game names
- `Character::new()` and `Item::new_random_*()` methods construct entities with sensible defaults

### **Singleton (Anti-Pattern, Pragmatic)**
- `GLOBAL_MESSAGES` static with `Lazy<Mutex<VecDeque<GameMessage>>>` provides thread-safe global state
- Necessary for pure-function game logic to emit events without explicit dependency injection

## Data & Control Flow

### **Input Boundary**
- **Entry Points**: 
  - `Game::new(name)` — Creates game instance
  - `Game::start()` — Initializes simulation
  - `Game::run_day_night_cycle(is_day: bool)` — Advances one half-day cycle
- **Configuration**: `GameConfig` struct centralizes magic numbers (`low_character_threshold`, `day_event_frequency`, etc.) with runtime-tunable knobs

### **Execution Flow**
```
run_day_night_cycle(day: bool)
  ├─> check_for_winner()
  │     └─> [adds winner/no-winner messages if game over]
  ├─> prepare_cycle(day)
  │     ├─> clear_messages() [if day cycle]
  │     ├─> increment day counter [if day cycle]
  │     └─> clear area events
  ├─> announce_cycle_start(day)
  │     └─> add_game_message(...) [day/night start, special events]
  ├─> do_a_cycle(day)
  │     ├─> announce_area_events() [closed areas and their hazards]
  │     ├─> ensure_open_area() [guarantee at least one safe zone]
  │     ├─> trigger_cycle_events(day, rng)
  │     │     ├─> spawn random AreaEvents [1/4 day, 1/8 night frequency]
  │     ├─> constrain_areas(rng) [close areas if <8 characters alive]
  │     ├─> run_character_cycle(day, rng, ...)
  │     │     ├─> pre-compute ActionSuggestions [day 1: Move]
  │     │     ├─> build area/character lookup HashMaps [optimization]
  │     │     └─> for each character:
  │     │           ├─> apply random CharacterEvent [based on luck]
  │     │           ├─> build EnvironmentContext [area details, closed areas]
  │     │           ├─> build EncounterContext [nearby characters, targets]
  │     │           └─> character.process_turn_phase(...) [delegates to characters module]
  │     ├─> process_alliance_events() [betrayal cascades, death sanity breaks]
  │     ├─> run_trauma_producers() [acquire/reinforce trauma afflictions]
  │     └─> spawn_patrons() [one per archetype, idempotent]
  ├─> announce_cycle_end(day)
  │     ├─> add_game_message(characters_left)
  │     ├─> announce recently dead characters
  │     └─> add_game_message(day/night end)
  └─> clean_up_recent_deaths()
        ├─> set day_killed statistics
        ├─> drop character items into their area
        └─> transition RecentlyDead -> Dead
```

### **Output Boundary**
- **Message System**: `messages.rs` functions (`add_game_message`, `add_area_message`, `add_character_message`) append to `GLOBAL_MESSAGES`
- **Queries**: 
  - `get_all_messages()` — Full event log
  - `get_messages_by_source(source)` — Filtered by source type
  - `get_messages_by_day(day)` — Filtered by game day
- **State Inspection**: `Game::living_characters()`, `Game::winner()`, public fields (`status`, `day`, `characters`, `areas`)

### **Random Number Generation**
- `SmallRng` from `rand` crate seeded per cycle (`SmallRng::from_rng(&mut rand::rng())`)
- Used for: area selection, event triggering, character shuffling, character AI decisions

## Integration Points

### **Consumed By**
- **API Crate** (`api/`): REST endpoints call `Game` methods to advance simulation and query state
- **Announcers Crate** (`announcers/`): Consumes messages from `GLOBAL_MESSAGES` to generate LLM commentary
- **Browser** (HTMX): Indirectly via API — API renders Maud templates for HTML display

### **Depends On**
- **Path crates** (extracted from `game/src/`):
  - `areas` — `Area` enum, `AreaDetails` struct, `AreaEvent` enum, hex topology
  - `characters` — `Character` struct, statuses, `Action` logic, combat, afflictions, alliances
  - `world` — terrain, items, output, clans, config, pathfinding, threats, naming, message helpers
- **Modules (within `game/src/`)**:
  - `events` — Typed `GameEvent` enum (serde-friendly counterpart to `GameOutput`)
  - `phases` — Per-phase pipeline scaffolding (environmental conditions, light levels)
  - `patrons` — Patron archetypes, budget bands, affinity tracking
  - `witty_phrase_generator` — Random name generation for games
- **External Crates**:
  - `rand` — RNG for procedural generation
  - `serde` — Serialization for API exposure
  - `shared` — Cross-crate types (`GameStatus`, `GameEvent`, `CombatBeat`, `Affliction`, etc.)
  - `uuid` — Unique identifiers for games and messages
  - `chrono` — Timestamps for messages
  - `once_cell` — Lazy static initialization of `GLOBAL_MESSAGES`

## Key Files

### **lib.rs** (18 lines)
Module aggregator. Declares `trauma_producers` and `witty_phrase_generator` as private.

### **games/mod.rs** (980 lines) — **Core Game State**
- **Purpose**: `Game` struct definition, lifecycle methods, state queries
- **Key Struct**: `Game`
  - Fields: `identifier`, `name`, `status`, `day`, `areas`, `characters`, `private`, `patrons`, `alliance_events`
  - Implements: `Default`, `Display`
- **Game Lifecycle**: `start()`, `end()`, `run_day_night_cycle()`
- **State Queries**: `living_characters()`, `winner()`, `random_open_area()`
- **Testing**: Submodules in `games/tests.rs` (1624 lines)

### **games/cycle.rs** (1270 lines) — **Cycle Execution**
- **Purpose**: Core cycle execution logic — building cycle context, executing character turns, processing area events
- **Key Functions**: `build_cycle_context()`, `execute_cycle()`, `run_character_cycle()`
- **Design**: Pre-computed immutable `CycleContext` split from mutable execution for borrow safety

### **games/alliances.rs** (258 lines) — **Alliance Event Processing**
- **Purpose**: Drains alliance event queue, applies betrayal cascades and death sanity breaks
- **Key Function**: `process_alliance_events()` — called between character turns

### **games/cycle_helpers.rs** (182 lines) — **Cycle Helper Methods**
- **Purpose**: Trauma producer invocation, area event announcements, event triggering
- **Key Functions**: `run_trauma_producers()`, `announce_area_events()`, `trigger_cycle_events()`

### **trauma_producers/** (835 lines) — **Trauma Producer Pipeline**
- **Purpose**: Scan the current phase's message log; acquire/reinforce trauma afflictions on witnesses/survivors (moved up from `characters/afflictions/producers/` — orchestration, not character logic)
- **Key Function**: `run_trauma_producers()` — gated on `game.config.trauma_enabled`

| File | Lines | Role |
|------|-------|------|
| `mod.rs` | 37 | Module aggregator |
| `shared.rs` | 145 | Shared producer utilities |
| `survive_betrayal.rs` | 44 | Betrayal survival trauma producer |
| `survive_near_death.rs` | 54 | Near-death survival trauma producer |
| `witness_ally_death.rs` | 66 | Ally death witness trauma producer |
| `witness_mass_casualty.rs` | 64 | Mass casualty witness trauma producer |
| `tests.rs` | 425 | Producer tests |

### **games/messages.rs** (142 lines) — **Message Helpers**
- **Purpose**: Fallback `MessagePayload` construction for legacy emission sites
- **Key Function**: `fallback_payload()` — transitional helper pending full typed payload migration

### **games/patrons.rs** (48 lines) — **Patron Spawning**
- **Purpose**: Spawn one patron per archetype with team-loyalist binding
- **Key Functions**: `spawn_patrons()`, `patron_affinity_snapshot()`

### **games/tests.rs** (1624 lines) — **Game Integration Tests**
- **Purpose**: Comprehensive test suite covering lifecycle, state transitions, area management, alliances, patrons

### **messages.rs** — moved out to shared + world
- Schema types (`MessageSource`, `GameMessage`, `MessagePayload`, `Phase`, refs): **`shared::messages`**
- `TaggedEvent` accumulator + terrain narrative helpers (`movement_narrative`, etc.): **`world::messages`**
- Log accumulation helpers (`get_all_messages()`, …) live on `Game` in `games/mod.rs`
- **Global State**: `GLOBAL_MESSAGES` (thread-safe `VecDeque<GameMessage>`)
- **API**:
  - Write: `add_message()`, `add_game_message()`, `add_area_message()`, `add_character_message()`
  - Read: `get_all_messages()`, `get_messages_by_source()`, `get_messages_by_day()`
  - Maintenance: `clear_messages()` (called at day start)
- **Thread Safety**: `Mutex` guards ensure concurrent access safety (future-proofing for multi-threaded API)

### **events/mod.rs** (654 lines) — **Event Module**
- **Purpose**: Module aggregator for typed `GameEvent` system; contains parity tests ensuring `GameEvent` renders identically to `GameOutput`
- **Key Exports**: `GameEvent` from `types.rs`

### **events/types.rs** (395 lines) — **Typed Game Events**
- **Purpose**: `GameEvent` enum — structured, owned, serde-friendly counterpart to `GameOutput`
- **Design**: Carries typed fields (UUIDs, names, items) so consumers react to *what happened* rather than re-parsing strings
- **Status**: Introduced in mqi.1; emission-site migration (mqi.2) and persistence (mqi.3) pending

### **events/display.rs** (517 lines) — **GameEvent Display**
- **Purpose**: `Display` implementation for `GameEvent` variants, rendering to the same strings as `GameOutput`

### **clans.rs** (201 lines) — **Team Colors & Terrain Affinity**
- **Purpose**: 8-color team palette + mascot epithets; per-goblin terrain affinity rolls
- **Key Struct**: `TeamColor` (name, hex)
- **Usage**: `character.team` picks the display color/epithet; `roll_terrain_affinity()` rolls 1–2 terrains from the goblin's own RNG

### **witty_phrase_generator/mod.rs** (260 lines) — **Name Generator**
- **Purpose**: Procedural game name generation using word combinations
- **Key Struct**: `WPGen`
  - Loads wordlists from embedded text files (`intensifiers.txt`, `adjectives.txt`, `nouns.txt`)
  - Methods: `with_words(n)`, `generic(...)`, `with_phrasewise_alliteration(...)`
- **Algorithm**: Backtracking constraint solver for length/alliteration requirements
- **Usage**: `Game::default()` calls `WPGen::new().with_words(3)` → e.g., "mighty-purple-dragon"

## Subdirectories

### Extracted crates (workspace split)

- `areas` crate — arena topology: see [areas/codemap.md](../../areas/codemap.md)
- `characters` crate — autonomous AI characters: see [characters/codemap.md](../../characters/codemap.md)
- `items`, `output`, `clans` — moved to the `world` crate

### **events/** (1566 lines total) — **Typed Event System**
Structured game events for persistence and analytics.

| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | 654 | Module aggregator, parity tests |
| `types.rs` | 395 | `GameEvent` enum (55+ typed variants) |
| `display.rs` | 517 | `Display` implementation for `GameEvent` |

### **patrons/** (601 lines total) — **Patron System**
Patron archetypes, budgets, and affinity tracking.

| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | 601 | `PatronContext`, archetype modifiers, patronship resolution |

### **phases/** (394 lines total) — **Phase Pipeline**
Per-phase environmental conditions and pipeline scaffolding.

| File | Lines | Purpose |
|------|-------|---------|
| `environment.rs` | 386 | `LightLevel`, `AfflictionDraft`, `AreaPhaseConditions` |
| `mod.rs` | 8 | Module aggregator |

## Notes
- **Pure Logic**: No file I/O, networking, or database access — all side effects are message emissions
- **Testability**: 60+ inline tests across modules using `rstest` parameterized testing; snapshot tests via `insta`
- **Performance**: `run_character_cycle()` uses `HashMap` lookups instead of nested loops (O(n²) → O(n))
- **Statefulness**: Only `GLOBAL_MESSAGES` persists between function calls; `Game` struct is fully serializable
- **Total LOC**: ~38,500 lines across 100+ source files
