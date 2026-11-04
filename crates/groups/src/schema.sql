-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE groups_local (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id BLOB NOT NULL UNIQUE CHECK(length(id)=16),
    home BLOB NOT NULL CHECK(length(home)=32),
    kind TEXT NOT NULL CHECK(kind IN ('clan','cohort')),
    created_by BLOB NOT NULL REFERENCES identity_accounts(id),
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    public_roster INTEGER NOT NULL CHECK(public_roster IN (0,1)),
    revision INTEGER NOT NULL CHECK(revision>0)
) STRICT;
CREATE INDEX groups_directory ON groups_local(home,kind,sequence);
CREATE TABLE groups_members (
    group_id BLOB NOT NULL REFERENCES groups_local(id) ON DELETE CASCADE,
    account BLOB NOT NULL REFERENCES identity_accounts(id),
    role TEXT NOT NULL CHECK(role IN ('creator','manager','member')),
    PRIMARY KEY(group_id,account)
) STRICT;
