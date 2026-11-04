-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE settings_site (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision > 0),
    welcome TEXT NOT NULL,
    artwork TEXT NOT NULL,
    menu_order TEXT NOT NULL
) STRICT;
