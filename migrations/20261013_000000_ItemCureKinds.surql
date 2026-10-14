-- Typed cure attributes on items: `cures` replaces the old name-substring
-- matching in apply_cure. Backfill legacy cure items so persisted bandages,
-- splints, and antibiotics keep their effect. Values mirror the
-- serde(rename_all = "snake_case") representation of CureKind.
UPDATE item SET cures = 'wounded' WHERE string::contains(string::lowercase(name), 'bandage');
UPDATE item SET cures = 'broken_bone' WHERE string::contains(string::lowercase(name), 'splint');
UPDATE item SET cures = 'infected' WHERE string::contains(string::lowercase(name), 'antibiotic');
