-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE profiles_local (
    account BLOB PRIMARY KEY REFERENCES identity_accounts(id),
    revision INTEGER NOT NULL CHECK(revision>0),
    public INTEGER NOT NULL CHECK(public IN (0,1)),
    bio TEXT NOT NULL,
    message TEXT NOT NULL,
    email TEXT NOT NULL,
    phone TEXT NOT NULL,
    contact TEXT NOT NULL,
    show_bio INTEGER NOT NULL CHECK(show_bio IN (0,1)),
    show_message INTEGER NOT NULL CHECK(show_message IN (0,1)),
    show_email INTEGER NOT NULL CHECK(show_email IN (0,1)),
    show_phone INTEGER NOT NULL CHECK(show_phone IN (0,1)),
    show_contact INTEGER NOT NULL CHECK(show_contact IN (0,1))
) STRICT;
