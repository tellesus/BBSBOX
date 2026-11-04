-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE settings_theme (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision > 0),
    background TEXT NOT NULL,
    text_color TEXT NOT NULL,
    accent TEXT NOT NULL,
    secondary_accent TEXT NOT NULL
) STRICT;
