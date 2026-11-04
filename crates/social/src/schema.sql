-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE social_follows (
    viewer BLOB NOT NULL REFERENCES identity_accounts(id),
    author BLOB NOT NULL REFERENCES identity_accounts(id),
    PRIMARY KEY(viewer,author),
    CHECK(viewer<>author)
) STRICT;
CREATE TABLE social_posts (
    id BLOB PRIMARY KEY CHECK(length(id)=32),
    author BLOB NOT NULL CHECK(length(author)=16),
    username TEXT NOT NULL,
    body TEXT NOT NULL,
    authored_at INTEGER NOT NULL,
    stream BLOB NOT NULL CHECK(length(stream)=32)
) STRICT;
CREATE INDEX social_author_posts ON social_posts(author,authored_at,id);
CREATE INDEX social_post_streams ON social_posts(stream);
CREATE TABLE social_forked_streams (stream BLOB PRIMARY KEY CHECK(length(stream)=32)) STRICT;
CREATE TABLE social_observations (
    event BLOB PRIMARY KEY CHECK(length(event)=32),
    first_seen_at INTEGER NOT NULL
) STRICT;
