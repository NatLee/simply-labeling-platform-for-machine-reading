import { Ban, Check, ClipboardCheck, ExternalLink, RotateCcw, Sparkles, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Link, useParams } from "react-router";
import { api } from "../api";
import { AnnotatorDialog } from "../components/AnnotatorPicker";
import { PageBody } from "../components/Layout";
import { Badge, Empty, ErrorBanner, Kbd, Loading, PageHeader, useToast, type BadgeTone } from "../components/ui";
import { useApp } from "../context";
import { answerEnd } from "../lib/drafts";
import { ORIGIN_LABEL, percent, STATUS_LABEL } from "../lib/format";
import { useAsync, useHotkeys } from "../lib/hooks";
import { segments, sliceChars, snippet, type Mark } from "../lib/span";
import { tone } from "../lib/tones";
import type { ReviewItem, ReviewStatus } from "../types";

const PAGE = 20;
const TABS: { value: ReviewStatus | undefined; label: string }[] = [
  { value: "pending", label: "待審核" },
  { value: "approved", label: "已通過" },
  { value: "rejected", label: "已退回" },
  { value: undefined, label: "全部" },
];
const STATUS_TONE: Record<ReviewStatus, BadgeTone> = { pending: "amber", approved: "green", rejected: "red" };

/** 答案附近的一段原文，答案用顏色標出來（每位標註者一個顏色）。 */
function Context({ text, item, annotatorOrder }: { text: string; item: ReviewItem; annotatorOrder: (id: number | null) => number }) {
  if (item.answers.length === 0) {
    return <p className="text-sm text-zinc-400">（沒有答案）</p>;
  }
  const first = Math.min(...item.answers.map((a) => a.start));
  const last = Math.max(...item.answers.map((a) => answerEnd(a)));
  const win = snippet(text, first, last);
  const marks: Mark[] = item.answers.map((a) => ({
    start: a.start - win.start,
    end: answerEnd(a) - win.start,
    tone: annotatorOrder(a.annotator_id),
  }));
  const segs = segments(sliceChars(text, win.start, win.end), marks);
  return (
    <p className="passage text-sm leading-7">
      {win.start > 0 && "…"}
      {segs.map((s) =>
        s.marks.length ? (
          <mark key={s.start} className={`rounded-[3px] px-px text-inherit ${tone(s.marks[0].tone).mark}`}>
            {s.text}
          </mark>
        ) : (
          <span key={s.start}>{s.text}</span>
        ),
      )}
      {win.end < Array.from(text).length && "…"}
    </p>
  );
}

export function Review() {
  const projectId = Number(useParams().pid);
  const { annotator, annotatorName } = useApp();
  const toast = useToast();
  const [status, setStatus] = useState<ReviewStatus | undefined>("pending");
  const [offset, setOffset] = useState(0);
  const [cursor, setCursor] = useState(0);
  const [notes, setNotes] = useState<Record<number, string>>({});
  const [askName, setAskName] = useState(false);
  const { data, error, loading, reload, setData } = useAsync(
    () => api.reviewQueue(projectId, { status, limit: PAGE, offset }),
    [projectId, status, offset],
  );
  useEffect(() => setCursor(0), [data?.items.length, status, offset]);

  // 每位標註者一個顏色，每頁重新分配
  const order = useMemo(() => {
    const ids: (number | null)[] = [];
    return (id: number | null) => {
      let i = ids.indexOf(id);
      if (i < 0) i = ids.push(id) - 1;
      return i;
    };
  }, [data]);

  const decide = async (item: ReviewItem, next: ReviewStatus) => {
    if (!annotator) {
      setAskName(true);
      return;
    }
    try {
      const q = await api.review(item.id, next, annotator.id, notes[item.id] ?? item.review_note);
      toast(next === "approved" ? "已通過" : next === "rejected" ? "已退回" : "改回待審核");
      if (status && status !== next) {
        // 不在這個分頁了：從列表拿掉，游標停在同一個位置
        setData((d) => (d ? { ...d, items: d.items.filter((x) => x.id !== item.id), total: d.total - 1 } : d));
      } else {
        setData((d) => (d ? { ...d, items: d.items.map((x) => (x.id === item.id ? { ...x, ...q } : x)) } : d));
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    }
  };

  const items = data?.items ?? [];
  const current = items[Math.min(cursor, items.length - 1)];
  useHotkeys([
    { key: "j", run: () => setCursor((c) => Math.min(c + 1, items.length - 1)) },
    { key: "k", run: () => setCursor((c) => Math.max(c - 1, 0)) },
    { key: "a", when: () => Boolean(current), run: () => current && void decide(current, "approved") },
    { key: "x", when: () => Boolean(current), run: () => current && void decide(current, "rejected") },
  ]);
  useEffect(() => {
    document.getElementById(`review-${current?.id}`)?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [current?.id]);

  return (
    <PageBody>
      <PageHeader
        title="審核"
        subtitle={
          <>
            看每一題的答案和標註者之間的一致程度，決定要不要放進資料集。快捷鍵：<Kbd>J</Kbd>/<Kbd>K</Kbd> 移動、
            <Kbd>A</Kbd> 通過、<Kbd>X</Kbd> 退回。
          </>
        }
      />
      <div className="mb-4 inline-flex max-w-full overflow-x-auto rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800">
        {TABS.map((t) => (
          <button
            key={t.label}
            onClick={() => {
              setStatus(t.value);
              setOffset(0);
            }}
            className={`rounded-md px-3 py-1.5 text-sm whitespace-nowrap ${
              status === t.value
                ? "bg-white font-medium text-brand-700 shadow-sm dark:bg-zinc-950 dark:text-brand-300"
                : "text-zinc-500 dark:text-zinc-400"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>
      {error && <ErrorBanner message={error} />}
      {loading && !data ? (
        <Loading />
      ) : items.length === 0 ? (
        <div className="card">
          <Empty icon={ClipboardCheck} title={status === "pending" ? "沒有待審核的題目" : "這裡沒有題目"}>
            {status === "pending" && "標註好的題目會出現在這裡。"}
          </Empty>
        </div>
      ) : (
        <div className="flex flex-col gap-3">
          {items.map((item, i) => (
            <article
              key={item.id}
              id={`review-${item.id}`}
              className={`card p-4 ${i === cursor ? "ring-2 ring-brand-500/70" : ""}`}
              onClick={() => setCursor(i)}
            >
              <div className="flex flex-wrap items-center gap-1.5">
                <Badge tone={STATUS_TONE[item.status]}>{STATUS_LABEL[item.status]}</Badge>
                {item.origin !== "human" && <Badge tone="sky">{ORIGIN_LABEL[item.origin]}</Badge>}
                {item.is_impossible && (
                  <Badge tone="red">
                    <Ban className="size-3" /> 無法回答
                  </Badge>
                )}
                {item.agreement && (
                  <Badge
                    tone={item.agreement.f1 >= 0.8 ? "green" : item.agreement.f1 >= 0.5 ? "amber" : "red"}
                    title={`完全一致 ${percent(item.agreement.exact)}`}
                  >
                    {item.agreement.annotators} 人・F1 {percent(item.agreement.f1)}
                  </Badge>
                )}
                <Link
                  to={`/p/${projectId}/annotate/${item.article_id}`}
                  className="ml-auto inline-flex items-center gap-1 text-xs text-zinc-500 hover:text-brand-700 dark:text-zinc-400"
                >
                  {item.article_title || `文章 #${item.article_id}`} <ExternalLink className="size-3" />
                </Link>
              </div>
              <h3 className="mt-2 font-medium">{item.text}</h3>
              <div className="mt-2 rounded-lg bg-zinc-50 px-3 py-2 dark:bg-zinc-950/60">
                <Context text={data!.contexts[String(item.article_id)] ?? ""} item={item} annotatorOrder={order} />
              </div>
              <ul className="mt-2 flex flex-wrap gap-1.5">
                {item.answers.map((a) => (
                  <li key={a.id} className={`inline-flex items-center gap-1.5 rounded-lg px-2 py-1 text-sm ${tone(order(a.annotator_id)).mark}`}>
                    {a.text}
                    <span className="font-mono text-[11px] text-zinc-500">{a.start}–{answerEnd(a)}</span>
                    <span className="text-[11px] text-zinc-500">· {annotatorName(a.annotator_id)}</span>
                    {(a.origin === "ai" || a.origin === "ai_edited") && <Sparkles className="size-3 text-brand-500" />}
                  </li>
                ))}
              </ul>
              {item.reference_answer && (
                <p className="mt-2 text-sm text-zinc-600 dark:text-zinc-300">
                  <span className="text-zinc-400">參考答案：</span>
                  {item.reference_answer}
                </p>
              )}
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <input
                  className="input max-w-sm flex-1 py-1.5"
                  placeholder="審核備註（選填）"
                  value={notes[item.id] ?? item.review_note}
                  onChange={(e) => setNotes((n) => ({ ...n, [item.id]: e.target.value }))}
                  onClick={(e) => e.stopPropagation()}
                />
                <span className="ml-auto flex gap-1.5">
                  {item.status !== "pending" && (
                    <button className="btn btn-ghost" onClick={() => void decide(item, "pending")}>
                      <RotateCcw className="size-4" /> 改回待審
                    </button>
                  )}
                  <button className="btn btn-secondary hover:text-rose-600" onClick={() => void decide(item, "rejected")}>
                    <X className="size-4" /> 退回
                  </button>
                  <button className="btn btn-success" onClick={() => void decide(item, "approved")}>
                    <Check className="size-4" /> 通過
                  </button>
                </span>
              </div>
              {item.reviewed_by !== null && item.status !== "pending" && (
                <p className="mt-2 text-xs text-zinc-400">由 {annotatorName(item.reviewed_by)} 審核</p>
              )}
            </article>
          ))}
          <div className="flex items-center justify-between text-sm text-zinc-500">
            <span className="tabular-nums">
              {offset + 1}–{offset + items.length}，共 {data!.total} 題
            </span>
            <span className="flex gap-2">
              <button className="btn btn-secondary py-1" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
                上一頁
              </button>
              <button className="btn btn-secondary py-1" disabled={offset + PAGE >= data!.total} onClick={() => setOffset(offset + PAGE)}>
                下一頁
              </button>
              <button className="btn btn-ghost py-1" onClick={() => void reload()}>
                重新整理
              </button>
            </span>
          </div>
        </div>
      )}
      <AnnotatorDialog open={askName} onClose={() => setAskName(false)} />
    </PageBody>
  );
}
