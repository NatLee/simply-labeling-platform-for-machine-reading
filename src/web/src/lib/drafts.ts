// 標註頁上每個問題的編輯狀態。存過的問題有 id；新的問題只在瀏覽器裡，按儲存才送出。

import type { Agreement, Origin, Question, ReviewStatus, SpanInput, Suggestion } from "../types";
import { charLength } from "./span";

export interface AnswerDraft {
  /** 存過的答案才有 id。 */
  id: number | null;
  start: number;
  text: string;
  annotatorId: number | null;
  origin: Origin;
}

export interface QuestionDraft {
  key: string;
  id: number | null;
  text: string;
  isImpossible: boolean;
  reference: string;
  answers: AnswerDraft[];
  createdBy: number | null;
  status: ReviewStatus | null;
  origin: Origin;
  agreement: Agreement | null;
  /** 問題文字、無法回答或參考答案改過，還沒存。新問題一律算。 */
  dirty: boolean;
  /** 從 AI 建議「修改」來的：儲存時用接受建議的 API，記下是 AI 建議改過的。 */
  suggestionId?: number;
  suggestionSpan?: SpanInput | null;
  /** 盲標：還沒作答前先藏起別人的答案。 */
  showOthers: boolean;
}

let seq = 0;
export function newKey(): string {
  seq += 1;
  return `new-${seq}`;
}

export function fromQuestion(q: Question): QuestionDraft {
  return {
    key: `q-${q.id}`,
    id: q.id,
    text: q.text,
    isImpossible: q.is_impossible,
    reference: q.reference_answer,
    answers: q.answers.map((a) => ({
      id: a.id,
      start: a.start,
      text: a.text,
      annotatorId: a.annotator_id,
      origin: a.origin,
    })),
    createdBy: q.created_by,
    status: q.status,
    origin: q.origin,
    agreement: q.agreement,
    dirty: false,
    showOthers: false,
  };
}

export function emptyDraft(createdBy: number | null): QuestionDraft {
  return {
    key: newKey(),
    id: null,
    text: "",
    isImpossible: false,
    reference: "",
    answers: [],
    createdBy,
    status: null,
    origin: "human",
    agreement: null,
    dirty: true,
    showOthers: true,
  };
}

/** AI 建議放進編輯區（「修改」）。 */
export function fromSuggestion(s: Suggestion, createdBy: number | null): QuestionDraft {
  const span = s.answer_start !== null && s.answer_text ? { start: s.answer_start, text: s.answer_text } : null;
  return {
    ...emptyDraft(createdBy),
    text: s.question,
    isImpossible: s.is_impossible,
    reference: s.reference_answer,
    answers: span ? [{ id: null, ...span, annotatorId: createdBy, origin: "ai" }] : [],
    origin: "ai",
    suggestionId: s.id,
    suggestionSpan: span,
  };
}

/** 有沒有還沒存的東西（離開這篇文章前要提醒）。 */
export function hasUnsaved(d: QuestionDraft): boolean {
  if (d.id === null) return d.text.trim() !== "" || d.answers.length > 0;
  return d.dirty;
}

/** 別人的答案要不要藏起來：問題是別人建的、我還沒作答，而且沒有按「顯示」。 */
export function hidesOthers(d: QuestionDraft, me: number | null): boolean {
  if (d.showOthers || d.id === null || d.isImpossible) return false;
  if (d.createdBy === me) return false;
  const mine = d.answers.some((a) => a.annotatorId === me);
  const others = d.answers.some((a) => a.annotatorId !== me);
  return others && !mine;
}

export function visibleAnswers(d: QuestionDraft, me: number | null): AnswerDraft[] {
  return hidesOthers(d, me) ? d.answers.filter((a) => a.annotatorId === me) : d.answers;
}

export function answerEnd(a: { start: number; text: string }): number {
  return a.start + charLength(a.text);
}

/** 答案型的 AI 建議（替已經存在的問題找答案）掛在那個問題上，其餘放右側面板。 */
export function splitSuggestions(
  suggestions: Suggestion[],
  drafts: QuestionDraft[],
): { forQuestion: Map<string, Suggestion[]>; panel: Suggestion[] } {
  const byText = new Map<string, QuestionDraft>();
  for (const d of drafts) if (d.id !== null) byText.set(d.text.trim(), d);
  const forQuestion = new Map<string, Suggestion[]>();
  const panel: Suggestion[] = [];
  for (const s of suggestions) {
    const target = byText.get(s.question.trim());
    if (target) forQuestion.set(target.key, [...(forQuestion.get(target.key) ?? []), s]);
    else panel.push(s);
  }
  return { forQuestion, panel };
}
