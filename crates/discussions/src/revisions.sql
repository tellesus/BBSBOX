-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE discussions_revisions (
    id BLOB PRIMARY KEY CHECK(length(id)=32),
    post BLOB NOT NULL REFERENCES discussions_entries(id),
    previous BLOB NOT NULL CHECK(length(previous)=32),
    body TEXT,
    withdrawn INTEGER NOT NULL CHECK(withdrawn IN (0,1)),
    stream BLOB NOT NULL CHECK(length(stream)=32),
    CHECK((withdrawn=1 AND body IS NULL) OR (withdrawn=0 AND body IS NOT NULL))
) STRICT;
CREATE INDEX discussions_revisions_stream ON discussions_revisions(stream,post);
CREATE TABLE discussions_heads (
    post BLOB PRIMARY KEY REFERENCES discussions_entries(id),
    revision BLOB NOT NULL REFERENCES discussions_revisions(id)
) STRICT;
