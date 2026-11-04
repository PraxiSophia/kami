-- Quarantined provider state; never a rollback anchor or a public key export API.
CREATE TABLE e2ee_account (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 version INTEGER NOT NULL CHECK(version=1),
 generation INTEGER NOT NULL CHECK(generation>0),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 state BLOB NOT NULL CHECK(length(state) BETWEEN 1 AND 1048576)
);
CREATE TABLE e2ee_session (
 id BLOB PRIMARY KEY CHECK(length(id)=32),
 account INTEGER NOT NULL DEFAULT 1 CHECK(account=1) REFERENCES e2ee_account(singleton),
 version INTEGER NOT NULL CHECK(version=1),
 generation INTEGER NOT NULL CHECK(generation>0),
 contact BLOB NOT NULL CHECK(length(contact)=32) REFERENCES contact_trust(contact),
 trust_generation INTEGER NOT NULL CHECK(trust_generation>0),
 pin BLOB NOT NULL CHECK(length(pin) BETWEEN 1 AND 512),
 context BLOB NOT NULL CHECK(length(context) BETWEEN 1 AND 2048),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 state BLOB NOT NULL CHECK(length(state) BETWEEN 1 AND 1048576)
) WITHOUT ROWID;
CREATE TABLE e2ee_frame (
 sender BLOB NOT NULL, id BLOB NOT NULL,
 session BLOB NOT NULL REFERENCES e2ee_session(id),
 direction INTEGER NOT NULL CHECK(direction IN (0,1)),
 session_generation INTEGER NOT NULL CHECK(session_generation>0),
 account_generation INTEGER NOT NULL CHECK(account_generation>0),
 session_digest BLOB NOT NULL CHECK(length(session_digest)=32),
 account_digest BLOB NOT NULL CHECK(length(account_digest)=32),
 frame BLOB NOT NULL CHECK(length(frame) BETWEEN 1 AND 65536),
 PRIMARY KEY(sender,id),
 FOREIGN KEY(sender,id) REFERENCES message(sender,id)
) WITHOUT ROWID;
