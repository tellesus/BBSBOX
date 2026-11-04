-- SPDX-License-Identifier: AGPL-3.0-or-later
CREATE TABLE mail_threads (
    id BLOB PRIMARY KEY CHECK(length(id)=16),
    root BLOB NOT NULL CHECK(length(root)=32),
    sender BLOB NOT NULL CHECK(length(sender)=16),
    recipient BLOB NOT NULL CHECK(length(recipient)=16),
    sender_name TEXT NOT NULL,
    recipient_name TEXT NOT NULL,
    subject TEXT NOT NULL,
    last_activity INTEGER NOT NULL,
    CHECK(sender<>recipient)
) STRICT;
CREATE INDEX mail_sender_box ON mail_threads(sender,last_activity,id);
CREATE INDEX mail_recipient_box ON mail_threads(recipient,last_activity,id);
CREATE TABLE mail_messages (
    id BLOB PRIMARY KEY CHECK(length(id)=32),
    thread BLOB NOT NULL REFERENCES mail_threads(id),
    parent BLOB REFERENCES mail_messages(id),
    sender BLOB NOT NULL CHECK(length(sender)=16),
    sender_name TEXT NOT NULL,
    body TEXT NOT NULL,
    authored_at INTEGER NOT NULL,
    stream BLOB NOT NULL CHECK(length(stream)=32)
) STRICT;
CREATE INDEX mail_thread_messages ON mail_messages(thread,authored_at,id);
CREATE INDEX mail_messages_stream ON mail_messages(stream,thread);
CREATE TABLE mail_quarantined (thread BLOB PRIMARY KEY CHECK(length(thread)=16)) STRICT;
