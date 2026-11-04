import type { Origin, ReviewStatus } from "../types";

export function percent(n: number | null | undefined, digits = 0): string {
  if (n === null || n === undefined || Number.isNaN(n)) return "—";
  return `${(n * 100).toFixed(digits)}%`;
}

export function ratio(part: number, whole: number): number {
  return whole > 0 ? part / whole : 0;
}

export function dateTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString("zh-TW", { dateStyle: "medium", timeStyle: "short", hour12: false });
}

export const ORIGIN_LABEL: Record<Origin, string> = {
  human: "人工",
  ai: "AI",
  ai_edited: "AI 修改",
  import: "匯入",
};

export const STATUS_LABEL: Record<ReviewStatus, string> = {
  pending: "待審核",
  approved: "已通過",
  rejected: "已退回",
};

/** 匯出格式的說明，順序和後端 `EXPORT_FORMATS` 一樣。 */
export const EXPORT_FORMAT_INFO: Record<string, { title: string; detail: string; ext: string }> = {
  "squad-v2": {
    title: "SQuAD 2.0 JSON",
    detail: "包含無法回答的題目和參考答案，所有欄位都保留，可以再匯入。",
    ext: ".json",
  },
  "squad-v1": {
    title: "SQuAD 1.1 JSON",
    detail: "只有可以回答的題目，給只支援 SQuAD 1.1 的工具。",
    ext: ".json",
  },
  jsonl: {
    title: "JSONL（Hugging Face）",
    detail: "squad／squad_v2 的欄位，一行一題，可以直接用 datasets 載入訓練。",
    ext: ".jsonl",
  },
  "rag-jsonl": {
    title: "RAG 評估 JSONL",
    detail: "Ragas 的欄位：user_input、reference、reference_contexts。",
    ext: ".jsonl",
  },
  "chat-jsonl": {
    title: "對話微調 JSONL",
    detail: "messages 格式（system、user、assistant），拿來微調 LLM。",
    ext: ".jsonl",
  },
};
