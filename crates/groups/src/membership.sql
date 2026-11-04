-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE groups_local ADD COLUMN membership_revision INTEGER NOT NULL DEFAULT 1 CHECK(membership_revision>0);
ALTER TABLE groups_local ADD COLUMN dissolved INTEGER NOT NULL DEFAULT 0 CHECK(dissolved IN (0,1));
CREATE TABLE groups_invites (
    group_id BLOB NOT NULL REFERENCES groups_local(id),
    account BLOB NOT NULL REFERENCES identity_accounts(id),
    invited_by BLOB NOT NULL REFERENCES identity_accounts(id),
    generation BLOB NOT NULL CHECK(length(generation)=16),
    PRIMARY KEY(group_id,account)
) STRICT;
CREATE INDEX groups_invite_recipient ON groups_invites(account,group_id);
