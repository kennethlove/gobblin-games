# characters crate — Autonomous AI Characters

Extracted from `game/src/characters/` during the workspace crate split. Deps: `areas`, `world`, `shared`.

Insta snapshot files renamed `game__characters__*` -> `characters__*`.

### **characters/** (crate root, was `game/src/characters/`) (12,648 lines total) — **Autonomous AI Characters**
AI-controlled characters with d20 combat, status effects, alliances, and context-aware decision-making.

| File/Dir | Lines | Purpose |
|----------|-------|---------|
| `mod.rs` | 2473 | `Character` struct, lifecycle methods, process_turn_phase |
| `tests.rs` | 710 | Character unit tests |
| `actions.rs` | 320 | `Action` enum, action selection, behavior definitions |
| `alliances.rs` | 523 | Alliance formation, breaks, event queue; MAX_ALLIES=5 |
| `combat_beat.rs` | 568 | Game-side narration for `CombatBeat` (wear, outcomes, stress) |
| `combat_tuning.rs` | 118 | `CombatTuning` — stress, stamina costs, band thresholds |
| `events.rs` | 159 | `CharacterEvent` enum, random event generation |
| `helpers.rs` | 183 | Utility functions for character calculations |
| `incidents.rs` | 654 | Sleep incidents, shelter-based rest, dormancy processing |
| `inventory.rs` | 400 | Item management, equip/unequip, durability tracking |
| `movement.rs` | 276 | Movement between areas, travel restrictions |
| `path.rs` | 209 | Per-character path planning (`AreaGraph`, `plan_path`) — moved from `areas/` |
| `rescue.rs` | 428 | Rescue resolution for Trapped afflictions |
| `stamina_band.rs` | 69 | `StaminaBand` derivation from stamina ratio (Fresh/Winded/Exhausted) |
| `statuses.rs` | 87 | `CharacterStatus` enum (Healthy/RecentlyDead/Dead/Mauled) |
| `survival.rs` | 327 | Hunger/thirst bands, survival mechanics, dehydration |
| `traits.rs` | 459 | `Trait` enum (25+ personality traits), trait bonuses |

### **characters/combat/** (2986 lines total) — **Combat Engine**
D20-based combat system with attack contests, wound infliction, and stress.

| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | 360 | Combat orchestrator, `Character::attacks()` |
| `resolve.rs` | 930 | Attack contest resolution, combat results application |
| `inflict_table.rs` | 536 | Wound infliction tables, severity rolls |
| `tests.rs` | 1133 | Combat integration tests |

### **characters/brains/** (3463 lines total) — **Character AI**
Decision-making engine with scoring, override layers for afflictions.

| File | Lines | Purpose |
|------|-------|---------|
| `mod.rs` | 952 | `Brain` struct, decision orchestration, scoring |
| `tests.rs` | 940 | Brain decision tests |
| `scoring.rs` | 170 | Action scoring heuristics |
| `decisions.rs` | 124 | Decision output types |
| `affliction_override.rs` | 341 | Override actions for active afflictions |
| `phobia_override.rs` | 367 | Override actions for phobia triggers |
| `fixation_override.rs` | 388 | Override actions for fixation processing |
| `addiction_override.rs` | 143 | Override actions for addiction cravings |
| `trauma_override.rs` | 108 | Override actions for trauma responses |

### **characters/afflictions/** (9523 lines total) — **Affliction System**
Comprehensive health condition system: anatomy, trauma, phobias, fixations, addictions.

| File/Dir | Lines | Purpose |
|----------|-------|---------|
| `mod.rs` | 202 | Module aggregator, acquisition API, tuning |
| `anatomy.rs` | 800 | `AcquireResolution`, body part targeting, wound application |
| `anatomy_tests.rs` | 590 | Anatomy resolution tests |
| `trauma.rs` | 312 | `TraumaAcquisition`, trauma producers |
| `trauma_tests.rs` | 319 | Trauma tests |
| `phobia/mod.rs` | 27 | Phobia module aggregator |
| `phobia/scan.rs` | 677 | Per-cycle phobia scan, trigger evaluation |
| `phobia/reaction.rs` | 475 | Phobia reaction effects, panic responses |
| `phobia/triggers.rs` | 384 | Trigger definitions, fear stimulus matching |
| `phobia/spawn.rs` | 199 | Spawn-time phobia acquisition |
| `phobia/outcomes.rs` | 116 | Phobia outcome resolution |
| `fixation.rs` | 924 | Fixation acquisition, processing, obsessions |
| `addiction.rs` | 369 | Addiction mechanics, craving system, decay |
| `addiction_tests.rs` | 410 | Addiction tests |
| `cascade.rs` | 477 | `CascadeResult`, affliction cascading |
| `cure.rs` | 346 | `CureOutcome`, recovery mechanics, item-to-cure mapping |
| `effects/mod.rs` | 13 | Effects module aggregator |
| `effects/brain_bias.rs` | 341 | `BrainBias` computation for afflictions |
| `effects/stat_modifiers.rs` | 436 | `StatModifiers` computation for afflictions |
| `effects/trauma_effects.rs` | 57 | Trauma-specific effect application |
| `trapped.rs` | 367 | Trapped affliction mechanics |
| `tuning.rs` | 45 | `AfflictionTuning` constants |
| `integration_tests.rs` | 1527 | Cross-module integration tests |
| `snapshot_tests.rs` | 93 | Snapshot regression tests |
| `trauma_snapshot_tests.rs` | 55 | Trauma snapshot tests |

### **characters/lifecycle/** (911 lines total) — **Character Lifecycle**
Death, health, stamina, and status management.

| File | Lines | Purpose |
|------|-------|---------|
| `status.rs` | 588 | Status transition logic, death processing |
| `health.rs` | 378 | Health pool management, healing, damage |
| `stamina.rs` | 139 | Stamina pool management, fatigue |
| `death.rs` | 102 | Death resolution, item drops |
| `mod.rs` | 4 | Module aggregator |

### **characters/snapshots/** and **characters/afflictions/snapshots/**
Snapshot test data directories (insta snapshot files for regression testing).

