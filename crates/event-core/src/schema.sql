-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE events_budget(singleton INTEGER PRIMARY KEY CHECK(singleton=1), bytes INTEGER NOT NULL CHECK(bytes>=0));
INSERT INTO events_budget VALUES(1,0);
CREATE TABLE events_raw(
 id BLOB PRIMARY KEY CHECK(length(id)=32), envelope BLOB NOT NULL CHECK(length(envelope)<=4096),
 body BLOB CHECK(length(body)<=65536), state INTEGER NOT NULL CHECK(state IN(0,2,3)),
 first_source TEXT NOT NULL, first_transfer TEXT NOT NULL,
 body_source TEXT, body_transfer TEXT,
 CHECK((body IS NULL AND body_source IS NULL AND body_transfer IS NULL) OR
       (body IS NOT NULL AND body_source IS NOT NULL AND body_transfer IS NOT NULL))
);
CREATE INDEX events_raw_pending ON events_raw(state,id);
CREATE TABLE events_links(
 id BLOB PRIMARY KEY REFERENCES events_raw(id), stream BLOB NOT NULL CHECK(length(stream)=32),
 sequence BLOB NOT NULL CHECK(length(sequence)=8), previous BLOB CHECK(length(previous)=32),
 resource TEXT NOT NULL, audience TEXT NOT NULL, distribution INTEGER NOT NULL CHECK(distribution IN(0,1)),
 connected INTEGER NOT NULL DEFAULT 0 CHECK(connected IN(0,1,2))
);
CREATE INDEX events_links_positions ON events_links(stream,sequence,id);
CREATE INDEX events_links_previous ON events_links(previous);
CREATE TABLE events_forks(stream BLOB PRIMARY KEY CHECK(length(stream)=32), first_sequence BLOB NOT NULL CHECK(length(first_sequence)=8));
