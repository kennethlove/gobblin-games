-- Adds the persisted survival-state fields used by the shelter +
-- hunger/thirst system (Plan 1: backend). All fields default-safe so
-- existing rows hydrate cleanly — pre-feature characters load as Sated and
-- exposed.
--
-- Mirrors the serde(default) annotations on `Character` in
-- game/src/characters/mod.rs.
DEFINE FIELD OVERWRITE hunger                  ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE thirst                  ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE sheltered_until         ON character TYPE option<int>;
DEFINE FIELD OVERWRITE starvation_drain_step   ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE dehydration_drain_step  ON character TYPE int DEFAULT 0;
