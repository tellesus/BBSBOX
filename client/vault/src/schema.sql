-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE client_vault (
    id INTEGER PRIMARY KEY CHECK(id=1),
    revision INTEGER NOT NULL CHECK(revision>0),
    record BLOB NOT NULL CHECK(length(record)<=8390000)
) STRICT;
