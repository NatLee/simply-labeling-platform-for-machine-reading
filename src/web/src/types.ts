// 和後端 `src/api/src/models.rs`、`store.rs` 回傳的形狀一致。

export type Origin = "human" | "ai" | "ai_edited" | "import";
export type ReviewStatus = "pending" | "approved" | "rejected";
export type SuggestionStatus = "open" | "accepted" | "rejected";

export interface Meta {
  version: string;
  ai: { enabled: true; provider: "anthropic" | "openai"; model: string } | { enabled: false; reason: string | null };
  export_formats: string[];
  max_upload_mb: number;
}

export interface Annotator {
  id: number;
  name: string;
  created_at: string;
}

export interface Project {
  id: number;
  name: string;
  description: string;
  created_at: string;
}

export interface ProjectSummary extends Project {
  articles: number;
  labeled_articles: number;
  questions: number;
  approved: number;
}

export interface AnnotatorStats {
  id: number;
  name: string;
  questions: number;
  answers: number;
  reviews: number;
}

export interface ProjectStats {
  articles: number;
  labeled_articles: number;
  questions: number;
  pending: number;
  approved: number;
  rejected: number;
  unanswerable: number;
  needs_answer: number;
  answers: number;
  multi_annotated: number;
  agreement_exact: number | null;
  agreement_f1: number | null;
  suggestions_open: number;
  suggestions_accepted: number;
  suggestions_rejected: number;
  annotators: AnnotatorStats[];
}

export interface Article {
  id: number;
  project_id: number;
  title: string;
  context: string;
  description: string;
  created_at: string;
}

export interface ArticleListItem {
  id: number;
  title: string;
  description: string;
  preview: string;
  length: number;
  questions: number;
  pending: number;
  approved: number;
  created_at: string;
}

export interface Page<T> {
  items: T[];
  total: number;
}

export interface Answer {
  id: number;
  question_id: number;
  /** 字元位置（Unicode code point），和 SQuAD 一樣。 */
  start: number;
  text: string;
  annotator_id: number | null;
  origin: Origin;
  created_at: string;
}

export interface Agreement {
  annotators: number;
  exact: number;
  f1: number;
}

export interface Question {
  id: number;
  article_id: number;
  text: string;
  is_impossible: boolean;
  reference_answer: string;
  origin: Origin;
  external_id: string | null;
  created_by: number | null;
  status: ReviewStatus;
  reviewed_by: number | null;
  reviewed_at: string | null;
  review_note: string;
  created_at: string;
  updated_at: string;
  answers: Answer[];
  agreement: Agreement | null;
}

export interface ArticleDetail {
  article: Article;
  questions: Question[];
  position: number;
  total: number;
  prev_id: number | null;
  next_id: number | null;
}

export interface Suggestion {
  id: number;
  article_id: number;
  question: string;
  is_impossible: boolean;
  answer_start: number | null;
  answer_text: string | null;
  reference_answer: string;
  provider: string;
  model: string;
  status: SuggestionStatus;
  question_id: number | null;
  requested_by: number | null;
  created_at: string;
  decided_at: string | null;
}

export interface Dropped {
  question: string;
  reason: string;
}

export interface SuggestResponse {
  suggestions: Suggestion[];
  dropped: Dropped[];
}

export interface ReviewItem extends Question {
  article_title: string;
}

export interface ReviewPage {
  items: ReviewItem[];
  contexts: Record<string, string>;
  total: number;
}

export interface ImportSummary {
  format: string;
  articles_created: number;
  articles_merged: number;
  questions_created: number;
  questions_skipped: number;
  answers_exact: number;
  answers_moved: number;
  answers_dropped: number;
  warnings: string[];
}

export interface SpanInput {
  start: number;
  text: string;
}
