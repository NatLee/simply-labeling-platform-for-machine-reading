//! 資料庫操作。API、匯入和舊版相容的路由都走這裡，規則（文章不重複、答案位置要對得上）只寫一次。

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use serde::Serialize;
use sqlx::{SqliteConnection, SqlitePool};

use crate::{
    dataset::{DatasetArticle, DatasetQa, Span},
    db,
    error::{AppError, AppResult},
    metrics::{self, Agreement},
    models::*,
    text::{self, OffsetUnit, Resolved},
};

/// 匯入時最多回報幾則警告，其餘只算數量。
const MAX_WARNINGS: usize = 20;

// ───────────────────────────── 專案和標註者 ─────────────────────────────

pub async fn project(db: &SqlitePool, id: i64) -> AppResult<Project> {
    sqlx::query_as::<_, Project>(
        "SELECT id, name, description, created_at FROM projects WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::not_found("找不到這個專案"))
}

pub async fn project_summaries(db: &SqlitePool) -> Result<Vec<ProjectSummary>> {
    Ok(sqlx::query_as::<_, ProjectSummary>(
        "SELECT p.id, p.name, p.description, p.created_at,
                (SELECT COUNT(*) FROM articles a WHERE a.project_id = p.id) AS articles,
                (SELECT COUNT(*) FROM articles a WHERE a.project_id = p.id
                   AND EXISTS (SELECT 1 FROM questions q WHERE q.article_id = a.id)) AS labeled_articles,
                (SELECT COUNT(*) FROM questions q JOIN articles a ON a.id = q.article_id
                   WHERE a.project_id = p.id) AS questions,
                (SELECT COUNT(*) FROM questions q JOIN articles a ON a.id = q.article_id
                   WHERE a.project_id = p.id AND q.status = 'approved') AS approved
         FROM projects p ORDER BY p.id",
    )
    .fetch_all(db)
    .await?)
}

pub async fn create_project(db: &SqlitePool, name: &str, description: &str) -> AppResult<Project> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::bad("專案名稱不能是空的"));
    }
    let res = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (name, description, created_at) VALUES (?, ?, ?)
         RETURNING id, name, description, created_at",
    )
    .bind(name)
    .bind(description.trim())
    .bind(db::now())
    .fetch_one(db)
    .await;
    match res {
        Ok(p) => Ok(p),
        Err(e) if is_unique_violation(&e) => {
            Err(AppError::conflict(format!("已經有叫「{name}」的專案")))
        }
        Err(e) => Err(e.into()),
    }
}

/// 名稱對應的專案；沒有就建立。匯入指令用。
pub async fn project_by_name_or_create(db: &SqlitePool, name: &str) -> AppResult<Project> {
    let existing = sqlx::query_as::<_, Project>(
        "SELECT id, name, description, created_at FROM projects WHERE name = ?",
    )
    .bind(name.trim())
    .fetch_optional(db)
    .await?;
    match existing {
        Some(p) => Ok(p),
        None => create_project(db, name, "").await,
    }
}

pub async fn annotator(db: &SqlitePool, id: i64) -> AppResult<Annotator> {
    sqlx::query_as::<_, Annotator>("SELECT id, name, created_at FROM annotators WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::bad("找不到這位標註者，請先在左下角選擇或新增標註者"))
}

pub async fn annotators(db: &SqlitePool) -> Result<Vec<Annotator>> {
    Ok(
        sqlx::query_as::<_, Annotator>("SELECT id, name, created_at FROM annotators ORDER BY name")
            .fetch_all(db)
            .await?,
    )
}

/// 新增標註者；同名的已經存在就直接回傳那一位（換電腦時輸入自己的名字就能接著用）。
pub async fn create_annotator(db: &SqlitePool, name: &str) -> AppResult<Annotator> {
    annotator_by_name_or_create(db, name).await
}

/// 名字對應的標註者，沒有就建立。帳號也用這個連到同名的標註者。
pub async fn annotator_by_name_or_create(db: &SqlitePool, name: &str) -> AppResult<Annotator> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::bad("名字不能是空的"));
    }
    sqlx::query("INSERT OR IGNORE INTO annotators (name, created_at) VALUES (?, ?)")
        .bind(name)
        .bind(db::now())
        .execute(db)
        .await?;
    Ok(
        sqlx::query_as::<_, Annotator>(
            "SELECT id, name, created_at FROM annotators WHERE name = ?",
        )
        .bind(name)
        .fetch_one(db)
        .await?,
    )
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.is_unique_violation())
}

// ───────────────────────────── 文章 ─────────────────────────────

pub enum Inserted {
    Created(i64),
    /// 專案裡已經有一樣的文章。
    Existing(i64),
}

impl Inserted {
    pub fn id(&self) -> i64 {
        match self {
            Self::Created(id) | Self::Existing(id) => *id,
        }
    }
}

/// 新增文章。換行先統一成 `\n`，同一個專案裡內容一樣的文章不會重複建立。
pub async fn insert_article(
    conn: &mut SqliteConnection,
    project_id: i64,
    title: &str,
    context: &str,
    description: &str,
    created_at: &str,
) -> AppResult<Inserted> {
    let context = text::normalize_newlines(context);
    if context.trim().is_empty() {
        return Err(AppError::bad("文章內容不能是空的"));
    }
    let hash = text::context_hash(&context);
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM articles WHERE project_id = ? AND context_sha256 = ?",
    )
    .bind(project_id)
    .bind(&hash)
    .fetch_optional(&mut *conn)
    .await?
    {
        return Ok(Inserted::Existing(id));
    }
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO articles (project_id, title, context, context_sha256, description, created_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(project_id)
    .bind(title.trim())
    .bind(&context)
    .bind(&hash)
    .bind(description.trim())
    .bind(created_at)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Inserted::Created(id))
}

const ARTICLE_COLUMNS: &str = "id, project_id, title, context, description, created_at";

pub async fn article(db: &SqlitePool, id: i64) -> AppResult<Article> {
    sqlx::query_as::<_, Article>(&format!(
        "SELECT {ARTICLE_COLUMNS} FROM articles WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::not_found("找不到這篇文章"))
}

/// 文章列表的篩選條件。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArticleFilter {
    /// 還沒有任何問題。
    Unlabeled,
    /// 有問題還沒審核。
    Pending,
    /// 有問題，而且都審核過了。
    Reviewed,
}

#[derive(Serialize)]
pub struct ArticlePage {
    pub items: Vec<ArticleListItem>,
    pub total: i64,
}

pub async fn list_articles(
    db: &SqlitePool,
    project_id: i64,
    search: Option<&str>,
    filter: Option<ArticleFilter>,
    limit: i64,
    offset: i64,
) -> Result<ArticlePage> {
    let filter_sql = match filter {
        None => "",
        Some(ArticleFilter::Unlabeled) => "AND questions = 0",
        Some(ArticleFilter::Pending) => "AND pending > 0",
        Some(ArticleFilter::Reviewed) => "AND questions > 0 AND pending = 0",
    };
    let search = search.map(str::trim).filter(|s| !s.is_empty());
    let pattern = search.map(|s| format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    let base = format!(
        "WITH stats AS (
            SELECT a.id, a.title, a.description, substr(a.context, 1, 160) AS preview,
                   length(a.context) AS length, a.created_at,
                   (SELECT COUNT(*) FROM questions q WHERE q.article_id = a.id) AS questions,
                   (SELECT COUNT(*) FROM questions q WHERE q.article_id = a.id AND q.status = 'pending') AS pending,
                   (SELECT COUNT(*) FROM questions q WHERE q.article_id = a.id AND q.status = 'approved') AS approved
            FROM articles a
            WHERE a.project_id = ?1
              AND (?2 IS NULL OR a.title LIKE ?2 ESCAPE '\\' OR a.context LIKE ?2 ESCAPE '\\'
                   OR a.description LIKE ?2 ESCAPE '\\')
         )
         SELECT * FROM stats WHERE 1 = 1 {filter_sql}"
    );
    let items =
        sqlx::query_as::<_, ArticleListItem>(&format!("{base} ORDER BY id LIMIT ?3 OFFSET ?4"))
            .bind(project_id)
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(db)
            .await?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM ({base})"))
        .bind(project_id)
        .bind(&pattern)
        .fetch_one(db)
        .await?;
    Ok(ArticlePage { items, total })
}

pub async fn article_detail(db: &SqlitePool, id: i64) -> AppResult<ArticleDetail> {
    let article = article(db, id).await?;
    let (position, total, prev_id, next_id): (i64, i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM articles WHERE project_id = ?1 AND id <= ?2),
                (SELECT COUNT(*) FROM articles WHERE project_id = ?1),
                (SELECT MAX(id) FROM articles WHERE project_id = ?1 AND id < ?2),
                (SELECT MIN(id) FROM articles WHERE project_id = ?1 AND id > ?2)",
    )
    .bind(article.project_id)
    .bind(id)
    .fetch_one(db)
    .await?;
    let questions = question_views(db, "q.article_id = ?", id).await?;
    Ok(ArticleDetail {
        article,
        questions,
        position,
        total,
        prev_id,
        next_id,
    })
}

/// 隨機挑一篇（舊版「Random pick other one!」）。`exclude` 是目前這篇，避免抽到同一篇。
pub async fn random_article(
    db: &SqlitePool,
    project_id: i64,
    exclude: Option<i64>,
    only_unlabeled: bool,
) -> Result<Option<Article>> {
    let unlabeled = if only_unlabeled {
        "AND NOT EXISTS (SELECT 1 FROM questions q WHERE q.article_id = articles.id)"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {ARTICLE_COLUMNS} FROM articles
         WHERE project_id = ? AND (? IS NULL OR id <> ?) {unlabeled}
         ORDER BY RANDOM() LIMIT 1"
    );
    let pick = |exclude: Option<i64>| {
        sqlx::query_as::<_, Article>(&sql)
            .bind(project_id)
            .bind(exclude)
            .bind(exclude)
            .fetch_optional(db)
    };
    // 專案裡只剩目前這一篇時，還是回傳它。
    Ok(match pick(exclude).await? {
        Some(a) => Some(a),
        None if exclude.is_some() => pick(None).await?,
        None => None,
    })
}

// ───────────────────────────── 問題和答案 ─────────────────────────────

pub struct NewQuestion<'a> {
    pub text: &'a str,
    pub is_impossible: bool,
    pub reference_answer: &'a str,
    pub origin: Origin,
    pub external_id: Option<&'a str>,
    pub created_by: Option<i64>,
    pub created_at: &'a str,
}

pub async fn insert_question(
    conn: &mut SqliteConnection,
    article_id: i64,
    q: &NewQuestion<'_>,
) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO questions (article_id, text, is_impossible, reference_answer, origin, external_id,
                                created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(article_id)
    .bind(q.text.trim())
    .bind(q.is_impossible)
    .bind(q.reference_answer.trim())
    .bind(q.origin)
    .bind(q.external_id)
    .bind(q.created_by)
    .bind(q.created_at)
    .bind(q.created_at)
    .fetch_one(&mut *conn)
    .await?)
}

/// 新增答案；同一個人已經標過一樣的範圍時回 `None`。位置要先用 [`check_span`] 檢查過。
pub async fn insert_answer(
    conn: &mut SqliteConnection,
    question_id: i64,
    span: &Span,
    annotator_id: Option<i64>,
    origin: Origin,
    created_at: &str,
) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "INSERT OR IGNORE INTO answers (question_id, start, text, annotator_id, origin, created_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(question_id)
    .bind(span.start as i64)
    .bind(&span.text)
    .bind(annotator_id)
    .bind(origin)
    .bind(created_at)
    .fetch_optional(&mut *conn)
    .await?)
}

/// 確認文章從 `start` 開始的文字正好是答案。後端不相信前端或模型算的位置。
pub fn check_span(context: &str, span: &SpanInput) -> AppResult<Span> {
    if span.text.trim().is_empty() {
        return Err(AppError::bad("答案不能是空的"));
    }
    if !text::span_matches(context, span.start, &span.text) {
        let actual =
            text::slice_chars(context, span.start, span.start + text::char_len(&span.text));
        return Err(AppError::bad(format!(
            "答案位置和文字對不上：文章第 {} 個字開始是「{actual}」，不是「{}」",
            span.start, span.text
        )));
    }
    Ok(Span {
        start: span.start,
        text: span.text.clone(),
    })
}

const QUESTION_COLUMNS: &str = "q.id, q.article_id, q.text, q.is_impossible, q.reference_answer, q.origin,
    q.external_id, q.created_by, q.status, q.reviewed_by, q.reviewed_at, q.review_note, q.created_at, q.updated_at";

pub async fn question(db: &SqlitePool, id: i64) -> AppResult<Question> {
    sqlx::query_as::<_, Question>(&format!(
        "SELECT {QUESTION_COLUMNS} FROM questions q WHERE q.id = ?"
    ))
    .bind(id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::not_found("找不到這個問題"))
}

pub async fn question_view(db: &SqlitePool, id: i64) -> AppResult<QuestionView> {
    question_views(db, "q.id = ?", id)
        .await?
        .pop()
        .ok_or_else(|| AppError::not_found("找不到這個問題"))
}

/// 依條件載入問題、答案，並算出標註者之間的一致程度。`condition` 是只含一個 `?` 的 SQL 條件。
async fn question_views(db: &SqlitePool, condition: &str, param: i64) -> Result<Vec<QuestionView>> {
    let questions = sqlx::query_as::<_, Question>(&format!(
        "SELECT {QUESTION_COLUMNS} FROM questions q WHERE {condition} ORDER BY q.id"
    ))
    .bind(param)
    .fetch_all(db)
    .await?;
    let answers = sqlx::query_as::<_, Answer>(&format!(
        "SELECT a.id, a.question_id, a.start, a.text, a.annotator_id, a.origin, a.created_at
         FROM answers a JOIN questions q ON q.id = a.question_id WHERE {condition} ORDER BY a.id"
    ))
    .bind(param)
    .fetch_all(db)
    .await?;
    Ok(attach_answers(questions, answers))
}

fn attach_answers(questions: Vec<Question>, answers: Vec<Answer>) -> Vec<QuestionView> {
    let mut by_question: HashMap<i64, Vec<Answer>> = HashMap::new();
    for a in answers {
        by_question.entry(a.question_id).or_default().push(a);
    }
    questions
        .into_iter()
        .map(|question| {
            let answers = by_question.remove(&question.id).unwrap_or_default();
            let agreement = if question.is_impossible {
                None
            } else {
                agreement_of(&answers)
            };
            QuestionView {
                question,
                answers,
                agreement,
            }
        })
        .collect()
}

/// 依標註者分組。沒有標註者的答案（例如從 SQuAD 匯入的）各自算一組。
fn agreement_of(answers: &[Answer]) -> Option<Agreement> {
    let mut groups: Vec<Vec<&str>> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    for a in answers {
        match a.annotator_id {
            Some(id) => {
                let i = *index.entry(id).or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
                groups[i].push(&a.text);
            }
            None => groups.push(vec![&a.text]),
        }
    }
    metrics::agreement(&groups)
}

// ───────────────────────────── 審核和列表 ─────────────────────────────

#[derive(Serialize)]
pub struct ReviewItem {
    #[serde(flatten)]
    pub view: QuestionView,
    pub article_title: String,
}

#[derive(Serialize)]
pub struct ReviewPage {
    pub items: Vec<ReviewItem>,
    /// 這一頁用到的文章全文（id → 內容），前端用來畫出答案在文章裡的位置。
    pub contexts: HashMap<i64, String>,
    pub total: i64,
}

pub async fn review_page(
    db: &SqlitePool,
    project_id: i64,
    status: Option<ReviewStatus>,
    limit: i64,
    offset: i64,
) -> Result<ReviewPage> {
    let questions = sqlx::query_as::<_, Question>(&format!(
        "SELECT {QUESTION_COLUMNS} FROM questions q JOIN articles a ON a.id = q.article_id
         WHERE a.project_id = ?1 AND (?2 IS NULL OR q.status = ?2)
         ORDER BY q.id LIMIT ?3 OFFSET ?4"
    ))
    .bind(project_id)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM questions q JOIN articles a ON a.id = q.article_id
         WHERE a.project_id = ?1 AND (?2 IS NULL OR q.status = ?2)",
    )
    .bind(project_id)
    .bind(status)
    .fetch_one(db)
    .await?;

    let ids: Vec<i64> = questions.iter().map(|q| q.id).collect();
    let mut answers = Vec::new();
    for chunk in ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT id, question_id, start, text, annotator_id, origin, created_at
             FROM answers WHERE question_id IN ({placeholders}) ORDER BY id"
        );
        let mut q = sqlx::query_as::<_, Answer>(&sql);
        for id in chunk {
            q = q.bind(id);
        }
        answers.extend(q.fetch_all(db).await?);
    }
    let article_ids: HashSet<i64> = questions.iter().map(|q| q.article_id).collect();
    let mut contexts = HashMap::new();
    let mut titles = HashMap::new();
    for id in article_ids {
        let (title, context): (String, String) =
            sqlx::query_as("SELECT title, context FROM articles WHERE id = ?")
                .bind(id)
                .fetch_one(db)
                .await?;
        titles.insert(id, title);
        contexts.insert(id, context);
    }
    let items = attach_answers(questions, answers)
        .into_iter()
        .map(|view| ReviewItem {
            article_title: titles
                .get(&view.question.article_id)
                .cloned()
                .unwrap_or_default(),
            view,
        })
        .collect();
    Ok(ReviewPage {
        items,
        contexts,
        total,
    })
}

// ───────────────────────────── 統計 ─────────────────────────────

#[derive(Debug, Default, Serialize, sqlx::FromRow)]
pub struct AnnotatorStats {
    pub id: i64,
    pub name: String,
    pub questions: i64,
    pub answers: i64,
    pub reviews: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectStats {
    pub articles: i64,
    pub labeled_articles: i64,
    pub questions: i64,
    pub pending: i64,
    pub approved: i64,
    pub rejected: i64,
    pub unanswerable: i64,
    /// 可以回答、但還沒有任何答案的問題（例如舊資料的答案位置找不回來）。
    pub needs_answer: i64,
    pub answers: i64,
    /// 有兩位以上標註者作答的問題。
    pub multi_annotated: i64,
    pub agreement_exact: Option<f64>,
    pub agreement_f1: Option<f64>,
    pub suggestions_open: i64,
    pub suggestions_accepted: i64,
    pub suggestions_rejected: i64,
    pub annotators: Vec<AnnotatorStats>,
}

pub async fn project_stats(db: &SqlitePool, project_id: i64) -> Result<ProjectStats> {
    let counts: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM articles WHERE project_id = ?1),
            (SELECT COUNT(*) FROM articles a WHERE a.project_id = ?1
               AND EXISTS (SELECT 1 FROM questions q WHERE q.article_id = a.id)),
            COUNT(q.id),
            COALESCE(SUM(q.status = 'pending'), 0),
            COALESCE(SUM(q.status = 'approved'), 0),
            COALESCE(SUM(q.status = 'rejected'), 0),
            COALESCE(SUM(q.is_impossible), 0),
            COALESCE(SUM(q.is_impossible = 0
               AND NOT EXISTS (SELECT 1 FROM answers x WHERE x.question_id = q.id)), 0),
            (SELECT COUNT(*) FROM answers x JOIN questions y ON y.id = x.question_id
               JOIN articles z ON z.id = y.article_id WHERE z.project_id = ?1)
         FROM questions q JOIN articles a ON a.id = q.article_id WHERE a.project_id = ?1",
    )
    .bind(project_id)
    .fetch_one(db)
    .await?;
    let (open, accepted, rejected): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(s.status = 'open'), 0), COALESCE(SUM(s.status = 'accepted'), 0),
                COALESCE(SUM(s.status = 'rejected'), 0)
         FROM suggestions s JOIN articles a ON a.id = s.article_id WHERE a.project_id = ?",
    )
    .bind(project_id)
    .fetch_one(db)
    .await?;
    let annotators = sqlx::query_as::<_, AnnotatorStats>(
        "SELECT n.id, n.name,
            (SELECT COUNT(*) FROM questions q JOIN articles a ON a.id = q.article_id
               WHERE a.project_id = ?1 AND q.created_by = n.id) AS questions,
            (SELECT COUNT(*) FROM answers x JOIN questions q ON q.id = x.question_id
               JOIN articles a ON a.id = q.article_id WHERE a.project_id = ?1 AND x.annotator_id = n.id) AS answers,
            (SELECT COUNT(*) FROM questions q JOIN articles a ON a.id = q.article_id
               WHERE a.project_id = ?1 AND q.reviewed_by = n.id) AS reviews
         FROM annotators n ORDER BY n.name",
    )
    .bind(project_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .filter(|a| a.questions + a.answers + a.reviews > 0)
    .collect();

    // 一致程度要看答案文字，在 Rust 裡算。
    let answers = sqlx::query_as::<_, Answer>(
        "SELECT x.id, x.question_id, x.start, x.text, x.annotator_id, x.origin, x.created_at
         FROM answers x JOIN questions q ON q.id = x.question_id JOIN articles a ON a.id = q.article_id
         WHERE a.project_id = ? AND q.is_impossible = 0 ORDER BY x.question_id",
    )
    .bind(project_id)
    .fetch_all(db)
    .await?;
    let mut by_question: HashMap<i64, Vec<Answer>> = HashMap::new();
    for a in answers {
        by_question.entry(a.question_id).or_default().push(a);
    }
    let agreements: Vec<Agreement> = by_question
        .values()
        .filter_map(|a| agreement_of(a))
        .collect();
    let mean = |f: fn(&Agreement) -> f64| {
        (!agreements.is_empty())
            .then(|| agreements.iter().map(f).sum::<f64>() / agreements.len() as f64)
    };

    Ok(ProjectStats {
        articles: counts.0,
        labeled_articles: counts.1,
        questions: counts.2,
        pending: counts.3,
        approved: counts.4,
        rejected: counts.5,
        unanswerable: counts.6,
        needs_answer: counts.7,
        answers: counts.8,
        multi_annotated: agreements.len() as i64,
        agreement_exact: mean(|a| a.exact),
        agreement_f1: mean(|a| a.f1),
        suggestions_open: open,
        suggestions_accepted: accepted,
        suggestions_rejected: rejected,
        annotators,
    })
}

// ───────────────────────────── 匯入 ─────────────────────────────

#[derive(Debug, Default, Serialize)]
pub struct ImportSummary {
    pub format: String,
    pub articles_created: usize,
    /// 專案裡已經有的文章，問題合併進去。
    pub articles_merged: usize,
    pub questions_created: usize,
    /// 同一篇文章已經有一樣的問題，略過。
    pub questions_skipped: usize,
    pub answers_exact: usize,
    /// 位置對不上，但在文章裡找到同樣的文字，改用找到的位置。
    pub answers_moved: usize,
    /// 文章裡找不到答案文字，沒有匯入。
    pub answers_dropped: usize,
    pub warnings: Vec<String>,
}

impl ImportSummary {
    fn warn(&mut self, msg: String) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(msg);
        }
    }
}

/// 把資料集寫進專案，全部在同一個交易裡：中途失敗就什麼都不留。
///
/// 同一個檔案匯入兩次不會重複：一樣的文章會合併，同一篇文章裡一樣的問題會略過。
/// 每個答案都會重新檢查位置（[`text::resolve_span`]）。
pub async fn import_dataset(
    db: &SqlitePool,
    project_id: i64,
    articles: Vec<DatasetArticle>,
    unit: OffsetUnit,
    origin: Origin,
    created_by: Option<i64>,
    format_label: &str,
) -> AppResult<ImportSummary> {
    let mut summary = ImportSummary {
        format: format_label.to_string(),
        ..Default::default()
    };
    let mut tx = db.begin().await?;
    let now = db::now();
    for art in articles {
        let context = text::normalize_newlines(&art.context);
        if context.trim().is_empty() {
            summary.warn(format!("「{}」的文章內容是空的，略過", art.title));
            continue;
        }
        let created_at = art.created_at.clone().unwrap_or_else(|| now.clone());
        let inserted = insert_article(
            &mut tx,
            project_id,
            &art.title,
            &context,
            &art.description,
            &created_at,
        )
        .await?;
        let article_id = inserted.id();
        let mut existing: HashSet<String> = match inserted {
            Inserted::Created(_) => {
                summary.articles_created += 1;
                HashSet::new()
            }
            Inserted::Existing(_) => {
                summary.articles_merged += 1;
                sqlx::query_scalar::<_, String>("SELECT text FROM questions WHERE article_id = ?")
                    .bind(article_id)
                    .fetch_all(&mut *tx)
                    .await?
                    .into_iter()
                    .collect()
            }
        };
        for qa in art.qas {
            let target = ImportTarget {
                article_id,
                context: &context,
                unit,
                origin,
                created_by,
                now: &now,
            };
            import_question(&mut tx, &mut summary, &mut existing, &target, qa).await?;
        }
    }
    tx.commit().await?;
    Ok(summary)
}

struct ImportTarget<'a> {
    article_id: i64,
    context: &'a str,
    unit: OffsetUnit,
    origin: Origin,
    /// 舊版 API 寫入時是登入的人；匯入檔案時是 None。
    created_by: Option<i64>,
    now: &'a str,
}

async fn import_question(
    conn: &mut SqliteConnection,
    summary: &mut ImportSummary,
    existing: &mut HashSet<String>,
    target: &ImportTarget<'_>,
    qa: DatasetQa,
) -> Result<()> {
    let question = qa.question.trim().to_string();
    if question.is_empty() {
        return Ok(());
    }
    if !existing.insert(question.clone()) {
        summary.questions_skipped += 1;
        return Ok(());
    }
    let created_at = qa.created_at.as_deref().unwrap_or(target.now);
    let question_id = insert_question(
        conn,
        target.article_id,
        &NewQuestion {
            text: &question,
            is_impossible: qa.is_impossible,
            reference_answer: &qa.reference_answer,
            origin: target.origin,
            external_id: qa.id.as_deref(),
            created_by: target.created_by,
            created_at,
        },
    )
    .await?;
    summary.questions_created += 1;
    for span in qa.answers {
        let answer = text::normalize_newlines(&span.text);
        let start = match text::resolve_span(target.context, span.start, target.unit, &answer) {
            Resolved::Exact(s) => {
                summary.answers_exact += 1;
                s
            }
            Resolved::Moved { from, to } => {
                summary.answers_moved += 1;
                summary.warn(format!(
                    "「{question}」的答案「{answer}」位置 {from} 對不上，改用 {to}"
                ));
                to
            }
            Resolved::NotFound => {
                summary.answers_dropped += 1;
                summary.warn(format!(
                    "「{question}」的答案「{answer}」不在文章裡，沒有匯入"
                ));
                continue;
            }
        };
        let span = Span {
            start,
            text: answer,
        };
        insert_answer(
            conn,
            question_id,
            &span,
            target.created_by,
            target.origin,
            created_at,
        )
        .await?;
    }
    Ok(())
}

// ───────────────────────────── 匯出 ─────────────────────────────

pub struct ExportData {
    pub articles: Vec<DatasetArticle>,
    /// 每題的匯出 id，和 `articles[i].qas[j]` 一一對應。
    pub ids: Vec<Vec<String>>,
    /// 可以回答但沒有答案的問題，不完整所以沒匯出。
    pub skipped_incomplete: usize,
}

/// 讀出要匯出的題目。退回的題目一律不匯出；`only_approved` 時只匯出審核通過的。
/// 沒有任何題目可匯出的文章不放進去。重複的答案（同位置同文字）只留一個。
pub async fn export_data(
    db: &SqlitePool,
    project_id: i64,
    only_approved: bool,
) -> Result<ExportData> {
    let status_sql = if only_approved {
        "q.status = 'approved'"
    } else {
        "q.status <> 'rejected'"
    };
    let articles = sqlx::query_as::<_, Article>(&format!(
        "SELECT {ARTICLE_COLUMNS} FROM articles WHERE project_id = ? ORDER BY id"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await?;
    let questions = sqlx::query_as::<_, Question>(&format!(
        "SELECT {QUESTION_COLUMNS} FROM questions q JOIN articles a ON a.id = q.article_id
         WHERE a.project_id = ? AND {status_sql} ORDER BY q.id"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await?;
    let answers = sqlx::query_as::<_, Answer>(&format!(
        "SELECT x.id, x.question_id, x.start, x.text, x.annotator_id, x.origin, x.created_at
         FROM answers x JOIN questions q ON q.id = x.question_id JOIN articles a ON a.id = q.article_id
         WHERE a.project_id = ? AND {status_sql} ORDER BY x.id"
    ))
    .bind(project_id)
    .fetch_all(db)
    .await?;

    let mut by_article: HashMap<i64, Vec<QuestionView>> = HashMap::new();
    for view in attach_answers(questions, answers) {
        by_article
            .entry(view.question.article_id)
            .or_default()
            .push(view);
    }
    let mut out = ExportData {
        articles: Vec::new(),
        ids: Vec::new(),
        skipped_incomplete: 0,
    };
    let mut used_ids: HashSet<String> = HashSet::new();
    for art in articles {
        let Some(views) = by_article.remove(&art.id) else {
            continue;
        };
        let mut qas = Vec::new();
        let mut ids = Vec::new();
        for v in views {
            let mut seen = HashSet::new();
            let spans: Vec<Span> = v
                .answers
                .iter()
                .filter(|a| seen.insert((a.start, a.text.clone())))
                .map(|a| Span {
                    start: a.start as usize,
                    text: a.text.clone(),
                })
                .collect();
            if !v.question.is_impossible && spans.is_empty() {
                out.skipped_incomplete += 1;
                continue;
            }
            let id = match v.question.external_id.as_deref().filter(|s| !s.is_empty()) {
                Some(ext) if used_ids.insert(ext.to_string()) => ext.to_string(),
                _ => {
                    let id = format!("q{}", v.question.id);
                    used_ids.insert(id.clone());
                    id
                }
            };
            ids.push(id);
            qas.push(DatasetQa {
                id: None,
                question: v.question.text,
                is_impossible: v.question.is_impossible,
                answers: spans,
                reference_answer: v.question.reference_answer,
                created_at: Some(v.question.created_at),
            });
        }
        if qas.is_empty() {
            continue;
        }
        out.ids.push(ids);
        out.articles.push(DatasetArticle {
            title: art.title,
            context: art.context,
            description: art.description,
            created_at: Some(art.created_at),
            qas,
        });
    }
    Ok(out)
}
