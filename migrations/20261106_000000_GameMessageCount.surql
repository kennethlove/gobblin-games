-- Rolling count of this game's persisted messages. save_game reads it
-- O(1) instead of scanning `message` with string::starts_with on every
-- advance (the scan grew with game age). DEFAULT 0 covers pre-migration
-- rows: the first save after migration falls back to the scan once and
-- materialises the real count.
DEFINE FIELD OVERWRITE message_count ON game TYPE int DEFAULT 0;
