# announcers/

## Responsibility

Structured commentary pipeline for Gobblin' Games events. Transforms typed game messages into deterministic narration by The Chronicler — template banks keyed by phase context, with no network or model dependency. The `Commentator` trait abstracts over any backend.

## Architecture

```
Phase events (Vec<GameMessage>)
         │
         ▼
BroadcastPackageBuilder::build(header, events, histories)
         │
         ▼
BroadcastPackage { header: GameStateSnapshot, events: Vec<EventLine>, histories: Vec<CharacterDigest> }
         │
         ▼
Commentator::generate(package) → CommentarySegment { lines: Vec<CommentaryLine> }
         │
         ▼
Persisted to SurrealDB (commentary_segments table) + pushed via SSE/WS
```

## Module Structure

| Module | File | Responsibility |
|---|---|---|
| `types` | `src/types.rs` | Core types: EventKind, EventLine, GameStateSnapshot, CharacterDigest, BroadcastPackage, CommentaryLine, CommentarySegment, CommentaryError |
| `severity` | `src/severity.rs` | Raw-value→narrative-descriptor mappings (damage, injury, hit quality, area activity) |
| `broadcast` | `src/broadcast.rs` | BroadcastPackageBuilder — iterates 55+ MessagePayload variants, produces typed EventLines |
| `history` | `src/history.rs` | CharacterHistories — rolling per-character digest (status, location, allies, notable events) |
| `commentator` | `src/commentator.rs` | Commentator trait (`async fn generate(&self, package) -> Result<CommentarySegment>`) |
| `chronicler` | `src/chronicler.rs` | The Chronicler — deterministic template narrator (same package, same lines) |

## Key Types

- **`BroadcastPackage`**: Full structured input to the narrator (header + events + histories)
- **`EventLine`**: Hybrid format — typed `EventKind` + prose + optional structured data
- **`CommentaryLine`**: One utterance (`speaker: String`, `text: String`)
- **`CommentarySegment`**: Persisted output with id, game_id, day, phase, lines, timestamp
- **`CharacterDigest`**: Rolling per-character summary (capped at 8 notable events)

## Integration

**API Trigger** (`api/src/games/mod.rs`):
- After `save_game()` drains phase messages, spawns `tokio::spawn` background task
- Builds `GameStateSnapshot` + `CharacterHistories` from current game state
- Calls `announcers::generate_commentary()`
- Persists `CommentarySegment` to SurrealDB (`commentary_segments` table)
- Broadcasts via `WebSocketMessage::Commentary` (relayed through both WebSocket and SSE)

**Narration Backend**:
- Default (and only): `Chronicler` — hash-picked template banks seeded by day/phase/alive count
- Deterministic: the same `BroadcastPackage` always yields the same lines
- Swappable: implement `Commentator` for other narration strategies

## Key Files
- `src/lib.rs`: Module declarations, re-exports, `generate_commentary()` convenience fn
- `src/types.rs`: All core data types
- `src/broadcast.rs`: MessagePayload → EventLine classification
- `src/history.rs`: Rolling per-character digest tracker
- `src/commentator.rs`: Commentator trait definition
- `src/chronicler.rs`: Deterministic Chronicler narrator (+ unit tests)
