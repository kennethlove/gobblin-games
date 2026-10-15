-- Arena selection at game creation: the resolved arena preset is
-- snapshotted on the game row; areas are instantiated from that
-- snapshot when the game is created. DEFAULT {} covers pre-migration
-- rows (their areas were created inline and stay as-is).
DEFINE FIELD OVERWRITE arena ON game TYPE object DEFAULT {};
DEFINE FIELD OVERWRITE arena.* ON game TYPE any;
