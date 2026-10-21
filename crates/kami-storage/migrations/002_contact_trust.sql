CREATE TABLE contact_trust (
    contact BLOB PRIMARY KEY NOT NULL CHECK(typeof(contact)='blob' AND length(contact)=32),
    format_version INTEGER NOT NULL CHECK(format_version=1),
    state INTEGER NOT NULL CHECK(state BETWEEN 0 AND 3),
    generation INTEGER NOT NULL CHECK(generation>=1),
    pinned BLOB CHECK(pinned IS NULL OR (typeof(pinned)='blob' AND length(pinned) BETWEEN 1 AND 512)),
    candidate BLOB NOT NULL CHECK(typeof(candidate)='blob' AND length(candidate) BETWEEN 1 AND 512),
    CHECK((state=0 AND pinned IS NULL) OR
          (state=1 AND pinned IS NOT NULL AND pinned=candidate) OR
          (state=2 AND pinned IS NOT NULL) OR state=3)
) STRICT;
