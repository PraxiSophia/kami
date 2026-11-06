-- Request authenticators and reservation tokens never leave encrypted storage custody.
CREATE TABLE sender_binding_key (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 key BLOB NOT NULL CHECK(length(key)=32)
);
CREATE TABLE sender_prepare (
 sender BLOB NOT NULL CHECK(length(sender)=32),
 id BLOB NOT NULL CHECK(length(id)=16),
 conversation BLOB NOT NULL CHECK(length(conversation)=16),
 recipient BLOB NOT NULL CHECK(length(recipient)=32),
 request_mac BLOB NOT NULL CHECK(length(request_mac)=32),
 sequence INTEGER NOT NULL CHECK(sequence>0),
 token BLOB NOT NULL CHECK(length(token)=32),
 expires INTEGER NOT NULL CHECK(expires>=0),
 state INTEGER NOT NULL CHECK(state IN (0,1,2)),
 PRIMARY KEY(sender,id)
) WITHOUT ROWID;
CREATE UNIQUE INDEX sender_prepare_lane ON sender_prepare(conversation,sender) WHERE state=0;
CREATE TABLE sender_attempt (
 sender BLOB NOT NULL, id BLOB NOT NULL,
 state INTEGER NOT NULL DEFAULT 0 CHECK(state IN (0,1,2,3,4)),
 attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0),
 trust_generation INTEGER NOT NULL CHECK(trust_generation>0),
 token BLOB CHECK(token IS NULL OR length(token)=32),
 deadline INTEGER NOT NULL DEFAULT 0 CHECK(deadline>=0),
 eligible INTEGER NOT NULL DEFAULT 0 CHECK(eligible>=0),
 failure INTEGER CHECK(failure IS NULL OR failure IN (0,1,2)),
 PRIMARY KEY(sender,id),
 FOREIGN KEY(sender,id) REFERENCES e2ee_frame(sender,id)
) WITHOUT ROWID;
