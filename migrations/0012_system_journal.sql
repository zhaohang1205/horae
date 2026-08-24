INSERT OR IGNORE INTO tasks (id, title, status, created_at, updated_at, archived_at, archive_reason) 
VALUES ('__journal__', 'System Journal', 'reference', 0, 0, 1, 'system');
