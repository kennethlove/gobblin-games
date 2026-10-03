DEFINE TABLE OVERWRITE commentary_segments SCHEMAFULL
    PERMISSIONS
        FOR select
            WHERE game_id IN (SELECT identifier FROM game WHERE created_by.id = $auth.id)
        FOR create, update, delete
            NONE; -- Server-side only

DEFINE FIELD OVERWRITE game_id ON commentary_segments TYPE string;
DEFINE FIELD OVERWRITE day ON commentary_segments TYPE int;
DEFINE FIELD OVERWRITE phase ON commentary_segments TYPE string;
DEFINE FIELD OVERWRITE lines ON commentary_segments TYPE array;
-- SurrealDB v3 checks object elements inside SCHEMAFULL arrays against
-- defined fields; without these two the CREATE rejects every segment.
DEFINE FIELD OVERWRITE lines[*].speaker ON commentary_segments TYPE string;
DEFINE FIELD OVERWRITE lines[*].text ON commentary_segments TYPE string;
-- serde emits chrono as an RFC3339 string, so accept both forms.
DEFINE FIELD OVERWRITE generated_at ON commentary_segments TYPE datetime | string;
DEFINE FIELD OVERWRITE model_used ON commentary_segments TYPE string;
DEFINE INDEX idx_commentary_game_day_phase ON commentary_segments COLUMNS game_id, day, phase UNIQUE;
