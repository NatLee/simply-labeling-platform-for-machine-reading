import { Check, ChevronDown, PencilLine, Sparkles, X } from "lucide-react";
import { useState } from "react";
import { useApp } from "../context";
import { answerEnd } from "../lib/drafts";
import type { Dropped, Suggestion } from "../types";
import { Badge, ErrorBanner, Kbd, Spinner } from "./ui";

const COUNTS = [3, 5, 8];

export function AiPanel({
  suggestions,
  dropped,
  busy,
  error,
  count,
  onCount,
  onGenerate,
  onAccept,
  onEdit,
  onReject,
  onHover,
}: {
  suggestions: Suggestion[];
  dropped: Dropped[] | null;
  busy: boolean;
  error: string | null;
  count: number;
  onCount: (n: number) => void;
  onGenerate: () => void;
  onAccept: (s: Suggestion) => void;
  onEdit: (s: Suggestion) => void;
  onReject: (s: Suggestion) => void;
  onHover: (s: Suggestion | null) => void;
}) {
  const { meta } = useApp();
  const ai = meta?.ai;
  const [showDropped, setShowDropped] = useState(false);

  return (
    <aside className="flex flex-col gap-4" aria-label="AI 建議">
      <div className="flex items-center gap-2">
        <span className="grid size-8 place-items-center rounded-lg bg-brand-600 text-white dark:bg-brand-500">
          <Sparkles className="size-4" />
        </span>
        <div className="min-w-0">
          <h2 className="text-sm font-semibold">AI 建議</h2>
          <p className="truncate text-xs text-zinc-500 dark:text-zinc-400">
            {ai?.enabled ? `${ai.provider === "anthropic" ? "Anthropic" : "OpenAI 相容"}・${ai.model}` : "未開啟"}
          </p>
        </div>
      </div>

      {!ai?.enabled ? (
        <div className="card p-4 text-sm text-zinc-600 dark:text-zinc-300">
          <p className="mb-2 font-medium">AI 輔助標注沒有開啟</p>
          <p className="mb-3 text-zinc-500 dark:text-zinc-400">{ai?.reason ?? "伺服器沒有回報 AI 狀態。"}</p>
          <p className="mb-1 text-xs text-zinc-500">在伺服器設定環境變數，例如：</p>
          <pre className="overflow-x-auto rounded-lg bg-zinc-100 p-2 font-mono text-[11px] leading-5 dark:bg-zinc-950">
            {`LLM_PROVIDER=anthropic\nANTHROPIC_API_KEY=…\n\n# 或 Ollama 等 OpenAI 相容服務\nLLM_PROVIDER=openai\nOPENAI_BASE_URL=http://ollama:11434/v1\nOPENAI_MODEL=qwen2.5:0.5b`}
          </pre>
        </div>
      ) : (
        <>
          <div className="card flex flex-col gap-3 p-4">
            <p className="text-sm text-zinc-600 dark:text-zinc-300">
              請模型讀這篇文章、出題並框出答案。答案一定是原文的片段，不是的會自動丟掉。
            </p>
            <div className="flex items-center gap-2">
              <div className="flex rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800" role="radiogroup" aria-label="題數">
                {COUNTS.map((n) => (
                  <button
                    key={n}
                    role="radio"
                    aria-checked={count === n}
                    onClick={() => onCount(n)}
                    className={`rounded-md px-2.5 py-1 text-xs font-medium ${
                      count === n
                        ? "bg-white text-brand-700 shadow-sm dark:bg-zinc-950 dark:text-brand-300"
                        : "text-zinc-500 dark:text-zinc-400"
                    }`}
                  >
                    {n} 題
                  </button>
                ))}
              </div>
              <button className="btn btn-primary flex-1" onClick={onGenerate} disabled={busy}>
                {busy ? <Spinner /> : <Sparkles className="size-4" />}
                {busy ? "產生中…" : "產生建議"}
                {!busy && <Kbd>G</Kbd>}
              </button>
            </div>
            {error && <ErrorBanner message={error} />}
            {dropped && dropped.length > 0 && (
              <div className="text-xs text-zinc-500 dark:text-zinc-400">
                <button className="inline-flex items-center gap-1 hover:underline" onClick={() => setShowDropped((v) => !v)}>
                  <ChevronDown className={`size-3.5 transition-transform ${showDropped ? "rotate-180" : ""}`} />
                  丟掉 {dropped.length} 個沒通過檢查的建議
                </button>
                {showDropped && (
                  <ul className="mt-1.5 space-y-1 pl-5">
                    {dropped.map((d, i) => (
                      <li key={i} className="list-disc">
                        {d.question || "（沒有問題）"}：{d.reason}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </div>

          {suggestions.length === 0 && !busy && (
            <p className="px-1 text-sm text-zinc-400 dark:text-zinc-500">還沒有建議。按「產生建議」開始。</p>
          )}

          <ul className="flex flex-col gap-3">
            {suggestions.map((s) => (
              <li
                key={s.id}
                className="card p-3.5 transition-shadow hover:shadow-md"
                onMouseEnter={() => onHover(s)}
                onMouseLeave={() => onHover(null)}
              >
                <p className="text-sm font-medium">{s.question}</p>
                <div className="mt-2 flex flex-wrap items-center gap-1.5 text-sm">
                  {s.is_impossible || s.answer_start === null || !s.answer_text ? (
                    <Badge tone="red">無法回答</Badge>
                  ) : (
                    <span className="inline-flex items-center gap-1.5 rounded-md border border-dashed border-brand-400 bg-brand-50 px-1.5 py-0.5 dark:bg-brand-950/50">
                      {s.answer_text}
                      <span className="font-mono text-[11px] text-zinc-500">
                        {s.answer_start}–{answerEnd({ start: s.answer_start, text: s.answer_text })}
                      </span>
                    </span>
                  )}
                </div>
                {s.reference_answer && (
                  <p className="mt-1.5 text-xs text-zinc-500 dark:text-zinc-400">參考答案：{s.reference_answer}</p>
                )}
                <div className="mt-3 flex gap-1.5">
                  <button className="btn btn-primary flex-1 px-2 py-1 text-xs" onClick={() => onAccept(s)}>
                    <Check className="size-3.5" /> 接受
                  </button>
                  <button className="btn btn-secondary px-2 py-1 text-xs" onClick={() => onEdit(s)}>
                    <PencilLine className="size-3.5" /> 修改
                  </button>
                  <button
                    className="btn btn-ghost px-2 py-1 text-xs hover:text-rose-600"
                    onClick={() => onReject(s)}
                    aria-label={`拒絕：${s.question}`}
                  >
                    <X className="size-3.5" /> 拒絕
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
    </aside>
  );
}
