-- 標注平台的資料表。時間一律存 UTC 的 RFC 3339 字串。
-- 答案位置（start）都是「字元」位置（Unicode code point），和 SQuAD、Python 的字串索引一樣。

CREATE TABLE projects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL
);

-- 標註者只有名字，沒有密碼（和舊版一樣不需要登入）。
CREATE TABLE annotators (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE articles (
    id             INTEGER PRIMARY KEY,
    project_id     INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title          TEXT NOT NULL DEFAULT '',
    context        TEXT NOT NULL,
    -- 同一個專案裡不放兩篇一樣的文章；重複匯入時靠它合併。
    context_sha256 TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL
);
CREATE UNIQUE INDEX articles_project_context ON articles(project_id, context_sha256);

CREATE TABLE questions (
    id               INTEGER PRIMARY KEY,
    article_id       INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    text             TEXT NOT NULL,
    -- SQuAD 2.0 的「無法回答」。這時 answers 存的是 plausible answers（看似答案的片段）。
    is_impossible    INTEGER NOT NULL DEFAULT 0,
    -- 自由作答的參考答案，給評估或微調 LLM 用；可以留空。
    reference_answer TEXT NOT NULL DEFAULT '',
    origin           TEXT NOT NULL DEFAULT 'human'
                     CHECK (origin IN ('human', 'ai', 'ai_edited', 'import')),
    -- 匯入的 SQuAD id，匯出時沿用。
    external_id      TEXT,
    created_by       INTEGER REFERENCES annotators(id) ON DELETE SET NULL,
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'approved', 'rejected')),
    reviewed_by      INTEGER REFERENCES annotators(id) ON DELETE SET NULL,
    reviewed_at      TEXT,
    review_note      TEXT NOT NULL DEFAULT '',
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);
CREATE INDEX questions_article ON questions(article_id);

CREATE TABLE answers (
    id           INTEGER PRIMARY KEY,
    question_id  INTEGER NOT NULL REFERENCES questions(id) ON DELETE CASCADE,
    start        INTEGER NOT NULL CHECK (start >= 0),
    text         TEXT NOT NULL CHECK (text <> ''),
    annotator_id INTEGER REFERENCES annotators(id) ON DELETE SET NULL,
    origin       TEXT NOT NULL DEFAULT 'human'
                 CHECK (origin IN ('human', 'ai', 'ai_edited', 'import')),
    created_at   TEXT NOT NULL
);
CREATE INDEX answers_question ON answers(question_id);
-- 同一個人不會在同一題標兩次一樣的範圍。
CREATE UNIQUE INDEX answers_unique ON answers(question_id, start, text, IFNULL(annotator_id, 0));

-- AI 建議：後端確認過答案是原文片段才會存進來，等人接受或拒絕。
CREATE TABLE suggestions (
    id               INTEGER PRIMARY KEY,
    article_id       INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    question         TEXT NOT NULL,
    is_impossible    INTEGER NOT NULL DEFAULT 0,
    answer_start     INTEGER,
    answer_text      TEXT,
    reference_answer TEXT NOT NULL DEFAULT '',
    provider         TEXT NOT NULL,
    model            TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'open'
                     CHECK (status IN ('open', 'accepted', 'rejected')),
    question_id      INTEGER REFERENCES questions(id) ON DELETE SET NULL,
    requested_by     INTEGER REFERENCES annotators(id) ON DELETE SET NULL,
    created_at       TEXT NOT NULL,
    decided_at       TEXT
);
CREATE INDEX suggestions_article ON suggestions(article_id, status);
