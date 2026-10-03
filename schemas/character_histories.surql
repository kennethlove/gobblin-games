DEFINE TABLE character_histories SCHEMAFULL
    PERMISSIONS
        FOR select
            WHERE game_id IN (SELECT identifier FROM game WHERE created_by.id = $auth.id)
        FOR create, update, delete
            NONE; -- Server-side only

DEFINE FIELD OVERWRITE game_id ON character_histories TYPE string;
DEFINE FIELD OVERWRITE digests ON character_histories TYPE array;
-- SurrealDB v3 checks object elements inside SCHEMAFULL arrays against
-- defined fields; the wildcard keeps digest fields open-ended.
DEFINE FIELD OVERWRITE digests[*].* ON character_histories TYPE any;
DEFINE INDEX idx_character_histories_game ON character_histories COLUMNS game_id UNIQUE;
