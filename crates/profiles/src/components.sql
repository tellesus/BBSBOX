-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE profiles_local ADD COLUMN show_feed INTEGER NOT NULL DEFAULT 0 CHECK(show_feed IN (0,1));
ALTER TABLE profiles_local ADD COLUMN show_events INTEGER NOT NULL DEFAULT 0 CHECK(show_events IN (0,1));
ALTER TABLE profiles_local ADD COLUMN show_games INTEGER NOT NULL DEFAULT 0 CHECK(show_games IN (0,1));
ALTER TABLE profiles_local ADD COLUMN show_drops INTEGER NOT NULL DEFAULT 1 CHECK(show_drops IN (0,1));
