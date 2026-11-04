-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE social_friends (
    first BLOB NOT NULL REFERENCES identity_accounts(id),
    second BLOB NOT NULL REFERENCES identity_accounts(id),
    requester BLOB NOT NULL REFERENCES identity_accounts(id),
    generation BLOB NOT NULL CHECK(length(generation)=16),
    confirmed INTEGER NOT NULL CHECK(confirmed IN (0,1)),
    PRIMARY KEY(first,second),
    CHECK(first<second),
    CHECK(requester IN (first,second))
) STRICT;
CREATE INDEX social_friends_second ON social_friends(second,first);
