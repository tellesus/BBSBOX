-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE discussions_entries (
    id BLOB PRIMARY KEY CHECK(length(id)=32),
    board TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('board','thread','reply')),
    author BLOB NOT NULL CHECK(length(author)=16),
    username TEXT NOT NULL,
    thread BLOB CHECK(length(thread)=32),
    parent BLOB CHECK(length(parent)=32),
    title TEXT,
    body TEXT,
    created_at INTEGER NOT NULL,
    stream BLOB NOT NULL CHECK(length(stream)=32),
    CHECK((kind='board' AND thread IS NULL AND parent IS NULL AND title IS NOT NULL AND body IS NULL)
       OR (kind='thread' AND thread=id AND parent IS NULL AND title IS NOT NULL AND body IS NOT NULL)
       OR (kind='reply' AND thread IS NOT NULL AND parent IS NOT NULL AND title IS NULL AND body IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX discussions_board_slug ON discussions_entries(board) WHERE kind='board';
CREATE INDEX discussions_by_board ON discussions_entries(board,kind,created_at,id);
CREATE INDEX discussions_by_thread ON discussions_entries(thread,created_at,id);
CREATE INDEX discussions_by_stream ON discussions_entries(stream,board);
CREATE TABLE discussions_quarantined (board TEXT PRIMARY KEY) STRICT;
