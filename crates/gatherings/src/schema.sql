-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE gatherings_current (
    id BLOB PRIMARY KEY CHECK(length(id)=16),
    revision BLOB NOT NULL CHECK(length(revision)=32),
    organizer BLOB NOT NULL CHECK(length(organizer)=16),
    username TEXT NOT NULL,
    details TEXT NOT NULL,
    starts_at INTEGER NOT NULL,
    cancelled INTEGER NOT NULL CHECK(cancelled IN (0,1))
) STRICT;
CREATE INDEX gatherings_by_start ON gatherings_current(starts_at,id);
CREATE INDEX gatherings_by_host ON gatherings_current(organizer,starts_at,id);
CREATE TABLE gatherings_revisions (
    event BLOB PRIMARY KEY CHECK(length(event)=32),
    gathering BLOB NOT NULL REFERENCES gatherings_current(id),
    stream BLOB NOT NULL CHECK(length(stream)=32)
) STRICT;
CREATE INDEX gatherings_by_stream ON gatherings_revisions(stream,gathering);
CREATE TABLE gatherings_quarantined (gathering BLOB PRIMARY KEY CHECK(length(gathering)=16)) STRICT;
