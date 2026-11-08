-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE private_forum_posts (
    event_id BLOB PRIMARY KEY CHECK(length(event_id)=32),
    forum TEXT NOT NULL,
    distribution INTEGER NOT NULL CHECK(distribution IN (0,1))
) STRICT;
CREATE INDEX private_forum_posts_scope ON private_forum_posts(forum,distribution,event_id);
