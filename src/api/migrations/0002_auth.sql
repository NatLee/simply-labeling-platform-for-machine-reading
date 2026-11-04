-- 內建登入（選用）：設了 ADMIN_PASSWORD 才會用到這兩張表。
-- 每個帳號對應一位同名的標註者，所以啟用登入前用某個名字標的資料，會歸到同名的帳號。

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (role IN ('admin', 'annotator', 'reviewer')),
    disabled      INTEGER NOT NULL DEFAULT 0,
    annotator_id  INTEGER NOT NULL UNIQUE REFERENCES annotators(id),
    created_at    TEXT NOT NULL
);

-- cookie 裡放隨機字串，資料庫只存它的 SHA-256，資料庫外流也拿不到可用的 session。
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX sessions_user ON sessions(user_id);
