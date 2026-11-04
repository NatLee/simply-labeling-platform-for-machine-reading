import { Ban, Check, Eye, MessageSquareText, Save, Sparkles, Trash2, X } from "lucide-react";
import { forwardRef } from "react";
import { useApp } from "../context";
import { answerEnd, hidesOthers, visibleAnswers, type QuestionDraft } from "../lib/drafts";
import { ORIGIN_LABEL, percent, STATUS_LABEL } from "../lib/format";
import { tone } from "../lib/tones";
import type { Suggestion } from "../types";
import { Badge, Kbd, Spinner, type BadgeTone } from "./ui";

const STATUS_TONE: Record<string, BadgeTone> = { pending: "amber", approved: "green", rejected: "red" };

export interface QuestionCardProps {
  draft: QuestionDraft;
  index: number;
  active: boolean;
  busy: boolean;
  aiEnabled: boolean;
  answerSuggestions: Suggestion[];
  onActivate: () => void;
  onChange: (patch: Partial<QuestionDraft>) => void;
  onRemoveAnswer: (answerIndex: number) => void;
  onSave: () => void;
  onDelete: () => void;
  onAskAi: () => void;
  onAcceptSuggestion: (s: Suggestion) => void;
  onRejectSuggestion: (s: Suggestion) => void;
}

export const QuestionCard = forwardRef<HTMLInputElement, QuestionCardProps>(function QuestionCard(
  {
    draft: d,
    index,
    active,
    busy,
    aiEnabled,
    answerSuggestions,
    onActivate,
    onChange,
    onRemoveAnswer,
    onSave,
    onDelete,
    onAskAi,
    onAcceptSuggestion,
    onRejectSuggestion,
  },
  inputRef,
) {
  const { annotator, annotatorName } = useApp();
  const me = annotator?.id ?? null;
  const hidden = hidesOthers(d, me);
  const answers = visibleAnswers(d, me);
  const hiddenCount = d.answers.length - answers.length;
  const t = tone(index);
  const saved = d.id !== null;
  const canSave = !saved || d.dirty;

  return (
    <div
      className={`card p-4 transition-shadow ${active ? "ring-2 ring-brand-500/70 dark:ring-brand-400/70" : "hover:shadow-md"}`}
      onClick={onActivate}
      data-testid={`question-${index}`}
    >
      <div className="mb-2 flex flex-wrap items-center gap-1.5">
        <span className={`size-2.5 rounded-full ${t.dot}`} aria-hidden />
        <span className="text-xs font-semibold text-zinc-500 dark:text-zinc-400">Q{index + 1}</span>
        {d.status && <Badge tone={STATUS_TONE[d.status]}>{STATUS_LABEL[d.status]}</Badge>}
        {!saved && <Badge tone="brand">{d.suggestionId ? "AI 建議（修改中）" : "未儲存"}</Badge>}
        {saved && d.dirty && <Badge tone="amber">有修改</Badge>}
        {saved && d.origin !== "human" && <Badge tone="sky">{ORIGIN_LABEL[d.origin]}</Badge>}
        {d.agreement && (
          <Badge
            tone={d.agreement.f1 >= 0.8 ? "green" : d.agreement.f1 >= 0.5 ? "amber" : "red"}
            title={`${d.agreement.annotators} 位標註者，完全一致 ${percent(d.agreement.exact)}`}
          >
            一致性 F1 {percent(d.agreement.f1)}
          </Badge>
        )}
        <span className="ml-auto" />
        <button
          className="btn btn-ghost px-1.5 py-1 text-zinc-400 hover:text-rose-600"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          title="刪除這個問題"
          aria-label="刪除這個問題"
        >
          <Trash2 className="size-4" />
        </button>
      </div>

      <input
        ref={inputRef}
        className="input text-[15px] font-medium"
        placeholder="輸入問題，例如：台北 101 有多高？"
        value={d.text}
        onFocus={onActivate}
        onChange={(e) => onChange({ text: e.target.value, dirty: true })}
        aria-label={`問題 ${index + 1}`}
      />

      <div className="mt-3 flex flex-wrap items-center gap-1.5" aria-label="答案">
        {d.isImpossible && (
          <Badge tone="red">
            <Ban className="size-3" />
            無法回答
          </Badge>
        )}
        {answers.map((a) => {
          const i = d.answers.indexOf(a);
          const mine = a.annotatorId === me;
          const removable = mine || a.annotatorId === null || !saved;
          return (
            <span
              key={`${a.start}-${a.text}-${a.annotatorId}`}
              className={`inline-flex max-w-full items-center gap-1.5 rounded-lg py-1 pr-1 pl-2 text-sm ${t.mark}`}
            >
              <span className="truncate">{a.text}</span>
              <span className="font-mono text-[11px] text-zinc-500 dark:text-zinc-400">
                {a.start}–{answerEnd(a)}
              </span>
              {!mine && <span className="text-[11px] text-zinc-500">· {annotatorName(a.annotatorId)}</span>}
              {a.origin !== "human" && a.origin !== "import" && <Sparkles className="size-3 text-brand-500" />}
              {removable && (
                <button
                  className="rounded p-0.5 text-zinc-500 hover:bg-white/60 hover:text-rose-600 dark:hover:bg-zinc-900/60"
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemoveAnswer(i);
                  }}
                  aria-label={`移除答案 ${a.text}`}
                >
                  <X className="size-3.5" />
                </button>
              )}
            </span>
          );
        })}
        {answers.length === 0 && !d.isImpossible && (
          <span className="text-sm text-zinc-400 dark:text-zinc-500">
            {hidden ? "先標你的答案（盲標），再看別人的。" : "在文章上選取答案，按"} {!hidden && <Kbd>Enter</Kbd>}{" "}
            {!hidden && "加入"}
          </span>
        )}
        {hiddenCount > 0 && (
          <button
            className="btn btn-ghost px-2 py-0.5 text-xs"
            onClick={(e) => {
              e.stopPropagation();
              onChange({ showOthers: true });
            }}
          >
            <Eye className="size-3.5" />
            顯示其他 {hiddenCount} 個答案
          </button>
        )}
      </div>

      {answerSuggestions.map((s) => (
        <div
          key={s.id}
          className="mt-3 flex flex-wrap items-center gap-2 rounded-lg border border-dashed border-brand-300 bg-brand-50/60 px-3 py-2 text-sm dark:border-brand-700 dark:bg-brand-950/40"
        >
          <Sparkles className="size-4 text-brand-500" />
          <span className="text-zinc-600 dark:text-zinc-300">AI 建議：</span>
          <span className="font-medium">{s.is_impossible ? "無法回答" : `「${s.answer_text}」`}</span>
          <span className="ml-auto flex gap-1">
            <button
              className="btn btn-primary px-2 py-1 text-xs"
              onClick={(e) => {
                e.stopPropagation();
                onAcceptSuggestion(s);
              }}
            >
              <Check className="size-3.5" /> 採用
            </button>
            <button
              className="btn btn-secondary px-2 py-1 text-xs"
              onClick={(e) => {
                e.stopPropagation();
                onRejectSuggestion(s);
              }}
            >
              不採用
            </button>
          </span>
        </div>
      ))}

      {(active || d.reference) && (
        <label className="mt-3 block">
          <span className="mb-1 flex items-center gap-1 text-xs font-medium text-zinc-500 dark:text-zinc-400">
            <MessageSquareText className="size-3.5" />
            參考答案（選填，自由作答，給 LLM 評估或微調用）
          </span>
          <textarea
            className="input min-h-9 resize-y py-1.5"
            rows={1}
            value={d.reference}
            placeholder="例如：台北 101 高 508 公尺。"
            onChange={(e) => onChange({ reference: e.target.value, dirty: true })}
          />
        </label>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-2">
        <label className="flex cursor-pointer items-center gap-1.5 text-sm text-zinc-600 select-none dark:text-zinc-300">
          <input
            type="checkbox"
            className="size-4 accent-brand-600"
            checked={d.isImpossible}
            onChange={(e) => onChange({ isImpossible: e.target.checked, dirty: true })}
          />
          無法回答 <span className="text-xs text-zinc-400">（SQuAD 2.0）</span>
        </label>
        <span className="ml-auto flex items-center gap-2">
          {aiEnabled && !d.isImpossible && (
            <button
              className="btn btn-ghost px-2 text-brand-700 dark:text-brand-300"
              disabled={busy || !d.text.trim()}
              onClick={(e) => {
                e.stopPropagation();
                onAskAi();
              }}
              title="請 AI 替這個問題找答案"
            >
              <Sparkles className="size-4" />
              AI 找答案
            </button>
          )}
          {canSave && (
            <button
              className="btn btn-primary"
              disabled={busy || !d.text.trim()}
              onClick={(e) => {
                e.stopPropagation();
                onSave();
              }}
            >
              {busy ? <Spinner /> : <Save className="size-4" />}
              儲存
              <span className="hidden text-[11px] opacity-75 sm:inline">Ctrl+Enter</span>
            </button>
          )}
        </span>
      </div>
    </div>
  );
});
