CREATE TABLE profile (singleton INTEGER PRIMARY KEY CHECK(singleton=1), namespace TEXT NOT NULL);
CREATE TABLE sender_sequence (
 conversation BLOB NOT NULL, sender BLOB NOT NULL, last INTEGER NOT NULL CHECK(last>0),
 PRIMARY KEY(conversation,sender)
) WITHOUT ROWID;
CREATE TABLE message (
 sender BLOB NOT NULL, id BLOB NOT NULL, conversation BLOB NOT NULL, sequence INTEGER NOT NULL CHECK(sequence>0),
 facts BLOB NOT NULL, direction INTEGER NOT NULL CHECK(direction IN (0,1)),
 state INTEGER NOT NULL DEFAULT 0 CHECK(state IN (0,1,2)),
 payload BLOB, received_at INTEGER NOT NULL DEFAULT 0, visible INTEGER NOT NULL DEFAULT 0 CHECK(visible IN (0,1)),
 PRIMARY KEY(sender,id), UNIQUE(conversation,sender,sequence)
) WITHOUT ROWID;
CREATE TABLE history (
 sender BLOB NOT NULL, id BLOB NOT NULL, payload BLOB NOT NULL,
 PRIMARY KEY(sender,id), FOREIGN KEY(sender,id) REFERENCES message(sender,id)
) WITHOUT ROWID;
CREATE TABLE receipt (
 sender BLOB NOT NULL, id BLOB NOT NULL, facts BLOB NOT NULL,
 PRIMARY KEY(sender,id), FOREIGN KEY(sender,id) REFERENCES message(sender,id)
) WITHOUT ROWID;
