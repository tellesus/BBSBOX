-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE gatherings_invites (
    gathering BLOB NOT NULL CHECK(length(gathering)=16),
    recipient BLOB NOT NULL CHECK(length(recipient)=16),
    organizer BLOB NOT NULL CHECK(length(organizer)=16),
    event BLOB NOT NULL UNIQUE CHECK(length(event)=32),
    sent_at INTEGER NOT NULL,
    stream BLOB NOT NULL CHECK(length(stream)=32),
    PRIMARY KEY(gathering,recipient),
    CHECK(recipient<>organizer)
) STRICT;
CREATE INDEX gatherings_invites_inbox ON gatherings_invites(recipient,sent_at,event);
CREATE INDEX gatherings_invites_stream ON gatherings_invites(stream);
CREATE TABLE gatherings_invite_forks (stream BLOB PRIMARY KEY CHECK(length(stream)=32)) STRICT;
