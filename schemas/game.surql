DEFINE TABLE OVERWRITE game SCHEMAFULL
    PERMISSIONS
        FOR select
            WHERE private = false
            OR created_by.id = $auth.id
        FOR create, update, delete
            WHERE created_by.id = $auth.id;

DEFINE FIELD OVERWRITE identifier ON game;
DEFINE FIELD OVERWRITE name ON game;
DEFINE FIELD OVERWRITE status ON game;
DEFINE FIELD OVERWRITE day ON game;
DEFINE FIELD OVERWRITE created_by ON game VALUE $auth READONLY;
DEFINE FIELD OVERWRITE private ON game DEFAULT true;

DEFINE INDEX OVERWRITE game_identifier ON game FIELDS identifier UNIQUE;

DEFINE TABLE OVERWRITE areas TYPE RELATION IN game OUT area ENFORCED
PERMISSIONS FOR
        SELECT, CREATE, UPDATE, DELETE WHERE $auth;

DEFINE INDEX OVERWRITE game_areas ON areas FIELDS game, area;

DEFINE FUNCTION OVERWRITE fn::get_full_game($game_id: string) {
    RETURN SELECT
        identifier,
        name,
        status,
        (day ?? 0) AS day,
        private,
        created_by,
        (
            SELECT *, ->owns->item[*] AS items
            FROM <-playing_in<-character[*]
            ORDER team
        )
        AS characters, (
            SELECT *, ->items->item[*] AS items
            FROM ->areas->area
        ) AS areas,
        count(SELECT id FROM <-playing_in<-character) AS character_count,
        count(<-playing_in<-character.id) == 24
        AND
        count(array::distinct(<-playing_in<-character.team)) == 8
        AS ready
    FROM game
    WHERE identifier = $game_id;
};

DEFINE FUNCTION OVERWRITE fn::get_display_game($game_id: string) {
    LET $characters = (
        SELECT  in.id,
                in.name,
                in.team,
                in.attributes.health
        FROM playing_in
        WHERE out.identifier = $game_id
    );

    LET $living_characters = (
        SELECT * FROM $characters.in WHERE attributes.health > 0
    );

    LET $winner = IF count($living_characters) == 1 THEN
        { identifier: $living_characters[0].identifier, name: $living_characters[0].name }
    ELSE
        NONE
    END;

    RETURN SELECT
        identifier,
        name,
        status,
        (day ?? 0) AS day,
        private,
        created_by.username,
        created_by.id == $auth.id AS is_mine,
        $characters.in AS characters,
        count($characters.in) as character_count,
        count(SELECT * FROM $characters.in WHERE attributes.health > 0) AS living_count,
        count($characters.in) == 24 AND
            count(array::distinct(<-playing_in<-character.team)) == 8 AS ready,
        $winner AS winner
    FROM game
    WHERE identifier = $game_id
    LIMIT 1
};

DEFINE FUNCTION OVERWRITE fn::get_list_games($limit: int, $offset: int) {
    RETURN SELECT name, identifier, status, (day ?? 0) AS day, private,
    created_by.id == $auth.id AS is_mine,
    created_by.username,
    count(SELECT id FROM <-playing_in<-character) AS character_count,
    count(SELECT id FROM <-playing_in<-character WHERE attributes.health > 0) AS living_count,
    count(<-playing_in<-character.id) == 24
    AND
    count(array::distinct(<-playing_in<-character.team)) == 8
    AS ready
    FROM game
    LIMIT $limit START $offset
};

DEFINE FUNCTION OVERWRITE fn::get_detail_game($game_id: string) {
    RETURN SELECT
        identifier,
        name,
        status,
        (day ?? 0) AS day,
        private,
        (
            SELECT *,
            ->owns->item[*] AS items
            FROM <-playing_in<-character[*]
            ORDER team
        ) AS characters, (
            SELECT *, ->items->item[*] AS items
            FROM ->areas->area
        ) AS areas,
        count(SELECT id FROM <-playing_in<-character) AS character_count,
        count(<-playing_in<-character.id) == 24 AND count(array::distinct(<-playing_in<-character.team)) == 8 AS ready,
        created_by.id == $auth.id AS is_mine,
        created_by.username
    FROM game
    WHERE identifier = $game_id
};

DEFINE FUNCTION OVERWRITE fn::get_characters_by_game($game_id: string) {
    RETURN SELECT (
        SELECT *, ->owns->item[*] AS items
        FROM <-playing_in<-character
        ORDER team
    ) AS characters
    FROM game WHERE identifier = $game_id
};

DEFINE FUNCTION OVERWRITE fn::get_characters_items_by_game($game_id: string) {
    RETURN SELECT <-playing_in<-character as character,
        <-playing_in<-character->owns->item as item,
        <-playing_in<-character->owns as owns
    FROM game WHERE identifier = $game_id
};

DEFINE FUNCTION OVERWRITE fn::get_areas_items_by_game($game_id: string) {
    RETURN SELECT ->areas->area AS area,
        ->areas->area->items->item AS item,
        ->areas->area->items AS items,
        -> areas AS areas
    FROM game WHERE identifier = $game_id;
};
