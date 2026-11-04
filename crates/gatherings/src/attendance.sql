-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE gatherings_rsvps (
    gathering BLOB NOT NULL CHECK(length(gathering)=16),
    account BLOB NOT NULL CHECK(length(account)=16),
    choice TEXT NOT NULL CHECK(choice IN ('going','maybe','not-going')),
    revision BLOB NOT NULL UNIQUE CHECK(length(revision)=32),
    PRIMARY KEY(gathering,account)
) STRICT;
CREATE TABLE gatherings_rsvp_revisions (
    revision BLOB PRIMARY KEY CHECK(length(revision)=32),
    gathering BLOB NOT NULL CHECK(length(gathering)=16),
    account BLOB NOT NULL CHECK(length(account)=16),
    stream BLOB NOT NULL CHECK(length(stream)=32)
) STRICT;
CREATE INDEX gatherings_rsvp_revision_stream ON gatherings_rsvp_revisions(stream,gathering,account);
CREATE TABLE gatherings_rsvp_forks (
    gathering BLOB NOT NULL CHECK(length(gathering)=16),
    account BLOB NOT NULL CHECK(length(account)=16),
    PRIMARY KEY(gathering,account)
) STRICT;
CREATE TABLE gatherings_reminders (
    account BLOB NOT NULL REFERENCES identity_accounts(id),
    gathering BLOB NOT NULL REFERENCES gatherings_current(id),
    minutes INTEGER NOT NULL CHECK(minutes IN (15,60,1440)),
    PRIMARY KEY(account,gathering)
) STRICT;
