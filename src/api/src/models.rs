//! 資料表對應的型別和 API 回傳的形狀。

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::metrics::Agreement;

/// 問題和答案從哪裡來。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum Origin {
    Human,
    /// AI 建議，人照原樣接受。
    Ai,
    /// AI 建議，人改過再接受。
    AiEdited,
    Import,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Open,
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

/// 專案列表上的一列，附簡單的進度。
#[derive(Clone, Debug, Serialize, FromRow)]
pub struct ProjectSummary {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub articles: i64,
    pub labeled_articles: i64,
    pub questions: i64,
    pub approved: i64,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Annotator {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Article {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub context: String,
    pub description: String,
    pub created_at: String,
}

/// 文章列表上的一列：不帶全文，只帶開頭和數量。
#[derive(Clone, Debug, Serialize, FromRow)]
pub struct ArticleListItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub preview: String,
    /// 字元數。
    pub length: i64,
    pub questions: i64,
    pub pending: i64,
    pub approved: i64,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Question {
    pub id: i64,
    pub article_id: i64,
    pub text: String,
    pub is_impossible: bool,
    pub reference_answer: String,
    pub origin: Origin,
    pub external_id: Option<String>,
    pub created_by: Option<i64>,
    pub status: ReviewStatus,
    pub reviewed_by: Option<i64>,
    pub reviewed_at: Option<String>,
    pub review_note: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Answer {
    pub id: i64,
    pub question_id: i64,
    /// 字元位置。
    pub start: i64,
    pub text: String,
    pub annotator_id: Option<i64>,
    pub origin: Origin,
    pub created_at: String,
}

/// 問題連同答案和標註者之間的一致程度。
#[derive(Clone, Debug, Serialize)]
pub struct QuestionView {
    #[serde(flatten)]
    pub question: Question,
    pub answers: Vec<Answer>,
    pub agreement: Option<Agreement>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ArticleDetail {
    pub article: Article,
    pub questions: Vec<QuestionView>,
    /// 在專案裡是第幾篇（從 1 開始），用來顯示「12 / 340」。
    pub position: i64,
    pub total: i64,
    pub prev_id: Option<i64>,
    pub next_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize, FromRow)]
pub struct Suggestion {
    pub id: i64,
    pub article_id: i64,
    pub question: String,
    pub is_impossible: bool,
    pub answer_start: Option<i64>,
    pub answer_text: Option<String>,
    pub reference_answer: String,
    pub provider: String,
    pub model: String,
    pub status: SuggestionStatus,
    pub question_id: Option<i64>,
    pub requested_by: Option<i64>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

/// 新增答案時送來的範圍。
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SpanInput {
    pub start: usize,
    pub text: String,
}
