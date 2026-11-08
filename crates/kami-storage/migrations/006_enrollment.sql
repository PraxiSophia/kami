-- Empty on legacy migration; no marker/key adoption or automatic backfill.
CREATE TABLE enrollment_complete (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    canonical BLOB NOT NULL CHECK(typeof(canonical)='blob' AND length(canonical)=66)
) STRICT;
CREATE TRIGGER enrollment_no_update BEFORE UPDATE ON enrollment_complete
BEGIN SELECT RAISE(ABORT, 'immutable enrollment'); END;
CREATE TRIGGER enrollment_no_delete BEFORE DELETE ON enrollment_complete
BEGIN SELECT RAISE(ABORT, 'immutable enrollment'); END;
