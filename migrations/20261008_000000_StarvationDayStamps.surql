-- Day-scaled survival drains: replaces the per-phase escalating step
-- counters with day stamps. HP drain now runs on configured survival-tick
-- hours as ceil(base x 1.25^(days-1)) damage units per day; eating or
-- drinking clears the stamp. Mirrors the serde(default) annotations on
-- `Character` in characters/src/lib.rs.
DEFINE FIELD OVERWRITE starving_since_day ON character TYPE option<int>;
DEFINE FIELD OVERWRITE dehydrated_since_day ON character TYPE option<int>;
REMOVE FIELD starvation_drain_step ON character;
REMOVE FIELD dehydration_drain_step ON character;
