-- SPDX-License-Identifier: AGPL-3.0-or-later
ALTER TABLE events_budget ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(typeof(revision)='integer' AND revision>=0);
CREATE INDEX events_receipts_module ON events_receipts(module,event_id);
