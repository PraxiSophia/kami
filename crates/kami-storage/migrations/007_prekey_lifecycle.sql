-- Public metadata is still stored only inside the SQLCipher database.
-- Reservations/tombstones survive expiry: an exposed OTK is never recycled.
CREATE TABLE prekey_clock(singleton INTEGER PRIMARY KEY CHECK(singleton=1), floor_ms INTEGER NOT NULL CHECK(floor_ms>=0), revoked_through INTEGER NOT NULL DEFAULT 0 CHECK(revoked_through>=0), retired_through INTEGER NOT NULL DEFAULT 0 CHECK(retired_through>=0));
INSERT INTO prekey_clock(singleton,floor_ms) VALUES(1,0);
CREATE TABLE prekey_inventory(
 reference BLOB PRIMARY KEY CHECK(length(reference) BETWEEN 1 AND 64),
 kind INTEGER NOT NULL CHECK(kind IN(1,2)),
 proof BLOB NOT NULL CHECK(length(proof) BETWEEN 1 AND 1024),
 bundle_generation INTEGER NOT NULL CHECK(bundle_generation>0),
 not_before_ms INTEGER NOT NULL CHECK(not_before_ms>=0),
 expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms>not_before_ms),
 revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN(0,1)),
 retired INTEGER NOT NULL DEFAULT 0 CHECK(retired IN(0,1)),
 consumed INTEGER NOT NULL DEFAULT 0 CHECK(consumed IN(0,1))
) STRICT;
CREATE TABLE prekey_publication(
 request BLOB PRIMARY KEY CHECK(length(request)=32),
 query BLOB NOT NULL CHECK(length(query) BETWEEN 1 AND 16380),
 peer BLOB NOT NULL CHECK(length(peer) BETWEEN 1 AND 128),
 device BLOB NOT NULL CHECK(length(device)=32),
 trust_generation INTEGER NOT NULL CHECK(trust_generation>0),
 pin BLOB NOT NULL CHECK(length(pin) BETWEEN 1 AND 512),
 account_generation INTEGER NOT NULL CHECK(account_generation>0),
 account_digest BLOB NOT NULL CHECK(length(account_digest)=32),
 reference BLOB NOT NULL REFERENCES prekey_inventory(reference),
 kind INTEGER NOT NULL CHECK(kind IN(1,2)),
 bundle_generation INTEGER NOT NULL CHECK(bundle_generation>0),
 response BLOB NOT NULL CHECK(length(response) BETWEEN 1 AND 16380),
 response_digest BLOB NOT NULL CHECK(length(response_digest)=32),
 not_before_ms INTEGER NOT NULL CHECK(not_before_ms>=0),
 expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms>not_before_ms),
 acknowledged INTEGER NOT NULL DEFAULT 0 CHECK(acknowledged IN(0,1)),
 revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN(0,1)),
 consumed INTEGER NOT NULL DEFAULT 0 CHECK(consumed IN(0,1))
) STRICT;
CREATE UNIQUE INDEX prekey_exclusive_otk ON prekey_publication(reference) WHERE kind=1;
CREATE TABLE prekey_session(
 session BLOB PRIMARY KEY CHECK(length(session)=32),
 request BLOB NOT NULL CHECK(length(request)=32),
 local INTEGER NOT NULL CHECK(local IN(0,1)),
 contact BLOB NOT NULL CHECK(length(contact)=32),
 reference BLOB NOT NULL CHECK(length(reference) BETWEEN 1 AND 64),
 kind INTEGER NOT NULL CHECK(kind IN(1,2)),
 response_digest BLOB NOT NULL CHECK(length(response_digest)=32)
) STRICT;
CREATE TABLE prekey_authority(
 device BLOB PRIMARY KEY CHECK(length(device)=32),
 binding_floor INTEGER NOT NULL CHECK(binding_floor>0),
 bundle_floor INTEGER NOT NULL CHECK(bundle_floor>0),
 account_floor INTEGER NOT NULL CHECK(account_floor>0),
 revoked_through INTEGER NOT NULL DEFAULT 0 CHECK(revoked_through>=0),
 retired_through INTEGER NOT NULL DEFAULT 0 CHECK(retired_through>=0)
) STRICT;
CREATE TABLE prekey_query(
 request BLOB PRIMARY KEY CHECK(length(request)=32),
 peer BLOB NOT NULL CHECK(length(peer) BETWEEN 1 AND 128),
 device BLOB NOT NULL CHECK(length(device)=32),
 trust_generation INTEGER NOT NULL CHECK(trust_generation>0),
 pin BLOB NOT NULL CHECK(length(pin) BETWEEN 1 AND 512),
 query BLOB NOT NULL CHECK(length(query) BETWEEN 1 AND 16380),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1))
) STRICT;
CREATE TRIGGER prekey_query_bound BEFORE INSERT ON prekey_query WHEN (SELECT count(*) FROM prekey_query)>=1024 BEGIN SELECT RAISE(ABORT,'prekey capacity'); END;
CREATE TABLE prekey_received(
 request BLOB PRIMARY KEY CHECK(length(request)=32),
 peer BLOB NOT NULL CHECK(length(peer) BETWEEN 1 AND 128),
 device BLOB NOT NULL CHECK(length(device)=32),
 trust_generation INTEGER NOT NULL CHECK(trust_generation>0),
 pin BLOB NOT NULL CHECK(length(pin) BETWEEN 1 AND 512),
 response BLOB NOT NULL CHECK(length(response) BETWEEN 1 AND 16380),
 ack BLOB NOT NULL CHECK(length(ack) BETWEEN 1 AND 16380),
 bundle_generation INTEGER NOT NULL CHECK(bundle_generation>0),
 expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms>0),
 completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN(0,1))
) STRICT;
-- Bound durable history without deleting monotonic floors or exposure evidence.
CREATE TRIGGER prekey_inventory_bound BEFORE INSERT ON prekey_inventory WHEN (SELECT count(*) FROM prekey_inventory)>=1024 BEGIN SELECT RAISE(ABORT,'prekey capacity'); END;
CREATE TRIGGER prekey_publication_bound BEFORE INSERT ON prekey_publication WHEN (SELECT count(*) FROM prekey_publication)>=1024 BEGIN SELECT RAISE(ABORT,'prekey capacity'); END;
CREATE TRIGGER prekey_received_bound BEFORE INSERT ON prekey_received WHEN (SELECT count(*) FROM prekey_received)>=1024 BEGIN SELECT RAISE(ABORT,'prekey capacity'); END;
CREATE TRIGGER prekey_authority_bound BEFORE INSERT ON prekey_authority WHEN (SELECT count(*) FROM prekey_authority)>=32 BEGIN SELECT RAISE(ABORT,'prekey capacity'); END;
