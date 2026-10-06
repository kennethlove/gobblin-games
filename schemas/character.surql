DEFINE TABLE OVERWRITE character SCHEMALESS
    PERMISSIONS
        FOR select FULL
        FOR create, update, delete WHERE created_by = $auth;

DEFINE FIELD OVERWRITE identifier ON character;
DEFINE FIELD OVERWRITE status ON character;
DEFINE FIELD OVERWRITE avatar ON character;
DEFINE FIELD OVERWRITE name ON character;
DEFINE FIELD OVERWRITE team ON character;
DEFINE FIELD OVERWRITE clan_name ON character TYPE string DEFAULT '';
DEFINE FIELD OVERWRITE area ON character;
DEFINE FIELD OVERWRITE player_name ON character;
DEFINE FIELD OVERWRITE statistics ON character;
DEFINE FIELD OVERWRITE attributes ON character;
DEFINE FIELD OVERWRITE stamina ON character;
DEFINE FIELD OVERWRITE max_stamina ON character;
DEFINE FIELD OVERWRITE terrain_affinity ON character;
DEFINE FIELD OVERWRITE allies ON character TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE traits ON character TYPE array<string> DEFAULT [];
DEFINE FIELD OVERWRITE turns_since_last_betrayal ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE hunger ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE thirst ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE sheltered_until ON character TYPE option<int>;
DEFINE FIELD OVERWRITE starvation_drain_step ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE dehydration_drain_step ON character TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE afflictions ON character TYPE option<array<object>>;
DEFINE FIELD OVERWRITE created_by ON character VALUE $auth READONLY;

DEFINE INDEX OVERWRITE character_identifier ON character FIELDS identifier UNIQUE;
DEFINE INDEX OVERWRITE character_created_by ON character FIELDS created_by;

DEFINE TABLE OVERWRITE owns TYPE RELATION IN character OUT item
PERMISSIONS
        FOR SELECT FULL
        FOR CREATE, UPDATE, DELETE WHERE $auth;

DEFINE INDEX OVERWRITE character_owns ON owns FIELDS character, item;

DEFINE TABLE OVERWRITE playing_in TYPE RELATION IN character OUT game ENFORCED
PERMISSIONS
        FOR SELECT FULL
        FOR CREATE, UPDATE, DELETE WHERE $auth;

DEFINE INDEX OVERWRITE character_playing_in ON playing_in FIELDS character, game;

DEFINE FUNCTION OVERWRITE fn::get_full_character($character_id: string) {
    RETURN SELECT *, ->owns->item[*] AS items,
    (SELECT * FROM fn::get_messages_by_character_id($character_id)) AS log,
    (->playing_in->game.status)[0] == "NotStarted" AS editable
    FROM character
    WHERE identifier = $character_id;
};
