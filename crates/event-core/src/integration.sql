-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE events_raw ADD COLUMN charged INTEGER NOT NULL DEFAULT 1 CHECK(charged IN(0,1));
CREATE TABLE events_receipts(
 event_id BLOB PRIMARY KEY REFERENCES events_raw(id), module TEXT NOT NULL,
 relevance INTEGER NOT NULL CHECK(relevance IN(0,1,2)), source TEXT NOT NULL, transfer TEXT NOT NULL
);
CREATE TABLE events_module_forks(
 module TEXT NOT NULL, stream BLOB NOT NULL, first_sequence BLOB NOT NULL,
 PRIMARY KEY(module,stream)
);
