-- These immutable artifacts reference canonical authenticated message/frame rows.
-- No duplicate facts, payload or mutable ratchet/session authority is introduced.
CREATE TABLE delivery_artifact (
 sender BLOB NOT NULL, id BLOB NOT NULL,
 direction INTEGER NOT NULL CHECK(direction IN (0,1)),
 frame BLOB NOT NULL CHECK(length(frame) BETWEEN 1 AND 65536),
 artifact_id BLOB NOT NULL CHECK(length(artifact_id)=32),
 session_generation INTEGER NOT NULL CHECK(session_generation>0),
 session_digest BLOB NOT NULL CHECK(length(session_digest)=32),
 PRIMARY KEY(sender,id),
 FOREIGN KEY(sender,id) REFERENCES e2ee_frame(sender,id)
) WITHOUT ROWID;
-- A gap is a presentation observation on a canonical accepted message.
-- 0 unresolved, 1 permanent (still reconcilable by a late predecessor).
CREATE TABLE receiver_gap (
 sender BLOB NOT NULL, id BLOB NOT NULL,
 state INTEGER NOT NULL CHECK(state IN (0,1)),
 PRIMARY KEY(sender,id),
 FOREIGN KEY(sender,id) REFERENCES message(sender,id)
) WITHOUT ROWID;
