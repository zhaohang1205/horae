-- UI/user settings (language, theme, ...) as a simple key-value table.
CREATE TABLE IF NOT EXISTS settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
