-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE files_areas (
    slug TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    download_role TEXT NOT NULL CHECK(download_role IN ('member','sysop')),
    upload_role TEXT NOT NULL CHECK(upload_role IN ('member','sysop')),
    quota INTEGER NOT NULL CHECK(quota>0),
    revision INTEGER NOT NULL CHECK(revision>0),
    used INTEGER NOT NULL DEFAULT 0 CHECK(used>=0 AND used<=quota),
    CHECK(download_role!='sysop' OR upload_role='sysop')
) STRICT;
CREATE TABLE files_entries (
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    id BLOB NOT NULL UNIQUE CHECK(length(id)=16),
    area TEXT NOT NULL REFERENCES files_areas(slug),
    owner BLOB NOT NULL REFERENCES identity_accounts(id),
    filename TEXT NOT NULL,
    media_type TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size>=0),
    digest TEXT NOT NULL REFERENCES core_blobs(digest)
) STRICT;
CREATE INDEX files_by_area ON files_entries(area,sequence);
