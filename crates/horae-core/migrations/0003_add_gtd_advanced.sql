-- Add GTD advanced fields
ALTER TABLE tasks ADD COLUMN delegated_to TEXT;
ALTER TABLE tasks ADD COLUMN project_type TEXT NOT NULL DEFAULT 'parallel';
ALTER TABLE tasks ADD COLUMN checklist TEXT NOT NULL DEFAULT '[]';
