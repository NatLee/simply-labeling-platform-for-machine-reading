import { useMemo, useRef } from "react";
import { segments, selectionToSpan, type CharSpan, type Mark } from "../lib/span";
import { SUGGESTION_MARK, tone } from "../lib/tones";

const PENDING_MARK = "bg-brand-500 text-white dark:bg-brand-400 dark:text-zinc-950";

function markClass(marks: Mark[]): string {
  if (marks.length === 0) return "";
  const pending = marks.find((m) => m.pending);
  if (pending) return PENDING_MARK;
  const suggestion = marks.find((m) => m.suggestion);
  if (suggestion) return SUGGESTION_MARK;
  const active = marks.find((m) => m.active);
  const base = active ? tone(active.tone).strong : tone(marks[0].tone).mark;
  // 好幾個答案重疊時加底線提示
  return marks.length > 1 ? `${base} underline decoration-2 decoration-zinc-500/60 underline-offset-4` : base;
}

/**
 * 顯示文章並標出答案。用滑鼠選取一段文字時，回報它的字元位置（已去掉前後空白）。
 * 容器裡只放文章文字，標記用 `<mark>` 包起來，所以選取範圍換算出的位置和文章一致。
 */
export function PassageView({
  text,
  marks,
  onSelect,
  onMarkClick,
}: {
  text: string;
  marks: Mark[];
  onSelect?: (span: CharSpan) => void;
  onMarkClick?: (ref: string) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const segs = useMemo(() => segments(text, marks), [text, marks]);

  const handleSelect = () => {
    if (!onSelect || !ref.current) return;
    const span = selectionToSpan(ref.current, text, window.getSelection());
    if (span) onSelect(span);
  };

  return (
    <div
      ref={ref}
      className="passage selection:bg-brand-200 dark:selection:bg-brand-500/50"
      onMouseUp={handleSelect}
      onKeyUp={(e) => e.shiftKey && handleSelect()}
      data-testid="passage"
    >
      {segs.map((s) =>
        s.marks.length === 0 ? (
          <span key={s.start}>{s.text}</span>
        ) : (
          <mark
            key={s.start}
            className={`rounded-[3px] px-px text-inherit transition-colors ${markClass(s.marks)} ${
              onMarkClick && s.marks.some((m) => m.ref) ? "cursor-pointer" : ""
            }`}
            data-start={s.start}
            onClick={() => {
              const target = s.marks.find((m) => m.ref && !m.pending);
              // 正在選取文字時不要搶走點擊
              if (target?.ref && onMarkClick && window.getSelection()?.isCollapsed !== false) {
                onMarkClick(target.ref);
              }
            }}
          >
            {s.text}
          </mark>
        ),
      )}
    </div>
  );
}
