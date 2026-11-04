import { ChevronLeft, ChevronRight, FileText, Keyboard, Plus, Shuffle, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api } from "../api";
import { AiPanel } from "../components/AiPanel";
import { AnnotatorDialog } from "../components/AnnotatorPicker";
import { PassageView } from "../components/PassageView";
import { QuestionCard } from "../components/QuestionCard";
import { Dialog, Empty, ErrorBanner, Kbd, Loading, useToast } from "../components/ui";
import { useApp } from "../context";
import {
  answerEnd,
  emptyDraft,
  fromQuestion,
  fromSuggestion,
  hasUnsaved,
  splitSuggestions,
  visibleAnswers,
  type QuestionDraft,
} from "../lib/drafts";
import { useHotkeys, useStoredState } from "../lib/hooks";
import type { CharSpan, Mark } from "../lib/span";
import type { ArticleDetail, Dropped, Question, Suggestion } from "../types";

/** `/p/:pid/annotate`：還沒指定文章時，挑一篇（優先挑還沒標註的）。 */
export function Annotate() {
  const { pid, aid } = useParams();
  const projectId = Number(pid);
  if (!aid) return <PickArticle projectId={projectId} />;
  return <Workspace key={aid} projectId={projectId} articleId={Number(aid)} />;
}

function PickArticle({ projectId }: { projectId: number }) {
  const navigate = useNavigate();
  const [empty, setEmpty] = useState(false);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const a = await api.randomArticle(projectId, { unlabeled: true }).catch(() => api.randomArticle(projectId, {}));
        if (!cancelled) navigate(`/p/${projectId}/annotate/${a.id}`, { replace: true });
      } catch {
        if (!cancelled) setEmpty(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectId, navigate]);
  if (!empty) return <Loading label="挑一篇文章…" />;
  return (
    <Empty icon={FileText} title="這個專案還沒有文章">
      先到 <Link className="text-brand-600 underline" to={`/p/${projectId}/articles`}>文章</Link> 新增一篇，或到{" "}
      <Link className="text-brand-600 underline" to={`/p/${projectId}/data`}>匯入／匯出</Link> 匯入 SQuAD 或舊版資料庫。
    </Empty>
  );
}

const SHORTCUTS: [string, string][] = [
  ["選取文字", "框選答案"],
  ["Enter", "把選取的文字加到目前的問題"],
  ["Esc", "取消選取"],
  ["N", "新增問題"],
  ["U", "目前的問題切換「無法回答」"],
  ["Ctrl + Enter", "儲存目前的問題"],
  ["G", "請 AI 產生建議"],
  ["← / →", "上一篇／下一篇"],
  ["R", "隨機換一篇"],
  ["?", "顯示這張表"],
];

function Workspace({ projectId, articleId }: { projectId: number; articleId: number }) {
  const { annotator, meta } = useApp();
  const me = annotator?.id ?? null;
  const navigate = useNavigate();
  const toast = useToast();
  const aiEnabled = Boolean(meta?.ai.enabled);

  const [detail, setDetail] = useState<ArticleDetail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [drafts, setDrafts] = useState<QuestionDraft[]>([]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const [pending, setPending] = useState<CharSpan | null>(null);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<Suggestion[]>([]);
  const [dropped, setDropped] = useState<Dropped[] | null>(null);
  const [aiBusy, setAiBusy] = useState(false);
  const [aiError, setAiError] = useState<string | null>(null);
  const [hovered, setHovered] = useState<Suggestion | null>(null);
  const [count, setCount] = useStoredState("mrc.aiCount", 5);
  const [unlabeledOnly, setUnlabeledOnly] = useStoredState("mrc.unlabeledOnly", false);
  const [showHelp, setShowHelp] = useState(false);
  const [askName, setAskName] = useState(false);
  const inputs = useRef(new Map<string, HTMLInputElement>());

  useEffect(() => {
    let cancelled = false;
    api
      .article(articleId)
      .then((d) => {
        if (cancelled) return;
        setDetail(d);
        const list = d.questions.map(fromQuestion);
        // 和舊版一樣，沒有問題時先放一列空白的，選了文字就能直接加答案。
        const initial = list.length > 0 ? list : [emptyDraft(me)];
        setDrafts(initial);
        setActiveKey(list.length > 0 ? null : initial[0].key);
      })
      .catch((e) => !cancelled && setLoadError(e.message));
    if (aiEnabled) api.suggestions(articleId).then((s) => !cancelled && setSuggestions(s)).catch(() => {});
    return () => {
      cancelled = true;
    };
    // 換標註者不重新載入文章，所以 me 不放進相依清單
  }, [articleId, aiEnabled]);

  const context = detail?.article.context ?? "";
  const active = drafts.find((d) => d.key === activeKey) ?? null;
  const { forQuestion, panel } = useMemo(() => splitSuggestions(suggestions, drafts), [suggestions, drafts]);

  const marks = useMemo<Mark[]>(() => {
    const out: Mark[] = [];
    drafts.forEach((d, i) => {
      for (const a of visibleAnswers(d, me)) {
        out.push({ start: a.start, end: answerEnd(a), tone: i, active: d.key === activeKey, ref: d.key });
      }
    });
    if (hovered?.answer_start != null && hovered.answer_text) {
      out.push({
        start: hovered.answer_start,
        end: answerEnd({ start: hovered.answer_start, text: hovered.answer_text }),
        tone: 0,
        suggestion: true,
      });
    }
    if (pending) out.push({ start: pending.start, end: pending.end, tone: 0, pending: true });
    return out;
  }, [drafts, activeKey, hovered, pending, me]);

  const patch = (key: string, p: Partial<QuestionDraft>) =>
    setDrafts((list) => list.map((d) => (d.key === key ? { ...d, ...p } : d)));

  /** 用伺服器回來的問題取代畫面上的草稿（存檔、加答案之後）。 */
  const replace = (key: string, q: Question) => {
    const next = { ...fromQuestion(q), showOthers: true };
    setDrafts((list) => {
      const exists = list.some((d) => d.key === key);
      return exists ? list.map((d) => (d.key === key ? next : d)) : [...list, next];
    });
    if (activeKey === key) setActiveKey(next.key);
  };

  const requireAnnotator = (): boolean => {
    if (me !== null) return true;
    setAskName(true);
    return false;
  };

  const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e), "error");

  const newQuestion = () => {
    const d = emptyDraft(me);
    setDrafts((list) => [...list, d]);
    setActiveKey(d.key);
    window.setTimeout(() => inputs.current.get(d.key)?.focus(), 0);
  };

  const addPending = async () => {
    if (!pending) return;
    const span = { start: pending.start, text: pending.text };
    let target = active;
    if (!target) {
      target = emptyDraft(me);
      const created = target;
      setDrafts((list) => [...list, created]);
      setActiveKey(created.key);
    }
    setPending(null);
    window.getSelection()?.removeAllRanges();
    if (target.answers.some((a) => a.start === span.start && a.text === span.text && a.annotatorId === me)) return;
    if (target.id === null) {
      const t = target;
      setDrafts((list) =>
        list.map((d) =>
          d.key === t.key ? { ...d, answers: [...d.answers, { id: null, ...span, annotatorId: me, origin: "human" }] } : d,
        ),
      );
      window.setTimeout(() => inputs.current.get(t.key)?.focus(), 0);
      return;
    }
    if (!requireAnnotator()) return;
    setBusyKey(target.key);
    try {
      replace(target.key, await api.addAnswer(target.id, span, me));
    } catch (e) {
      fail(e);
    } finally {
      setBusyKey(null);
    }
  };

  const removeAnswer = async (d: QuestionDraft, i: number) => {
    const a = d.answers[i];
    if (a.id === null) {
      patch(d.key, { answers: d.answers.filter((_, j) => j !== i) });
      return;
    }
    setBusyKey(d.key);
    try {
      await api.deleteAnswer(a.id);
      replace(d.key, await api.question(d.id!));
    } catch (e) {
      fail(e);
    } finally {
      setBusyKey(null);
    }
  };

  const save = async (d: QuestionDraft) => {
    if (!d.text.trim() || !requireAnnotator()) return;
    setBusyKey(d.key);
    try {
      let q: Question;
      const spans = d.answers.map((a) => ({ start: a.start, text: a.text }));
      if (d.id === null && d.suggestionId) {
        q = await api.acceptSuggestion(d.suggestionId, {
          annotator_id: me,
          question: d.text,
          is_impossible: d.isImpossible,
          reference_answer: d.reference,
          answers: spans,
        });
        setSuggestions((list) => list.filter((s) => s.id !== d.suggestionId));
      } else if (d.id === null) {
        q = await api.createQuestion(articleId, {
          text: d.text,
          is_impossible: d.isImpossible,
          reference_answer: d.reference,
          answers: spans,
          annotator_id: me,
        });
      } else {
        q = await api.updateQuestion(d.id, { text: d.text, is_impossible: d.isImpossible, reference_answer: d.reference });
      }
      replace(d.key, q);
      toast("已儲存");
    } catch (e) {
      fail(e);
    } finally {
      setBusyKey(null);
    }
  };

  const remove = async (d: QuestionDraft) => {
    if (d.id !== null && !window.confirm(`刪除問題「${d.text}」和它的所有答案？`)) return;
    try {
      if (d.id !== null) await api.deleteQuestion(d.id);
      setDrafts((list) => list.filter((x) => x.key !== d.key));
      if (activeKey === d.key) setActiveKey(null);
    } catch (e) {
      fail(e);
    }
  };

  const generate = async () => {
    if (!aiEnabled || aiBusy) return;
    setAiBusy(true);
    setAiError(null);
    try {
      const res = await api.generateSuggestions(articleId, count, me);
      setSuggestions((list) => [...list, ...res.suggestions]);
      setDropped(res.dropped);
      if (res.suggestions.length === 0) setAiError("模型這次沒有給出能用的建議，可以再試一次。");
    } catch (e) {
      setAiError(e instanceof Error ? e.message : String(e));
    } finally {
      setAiBusy(false);
    }
  };

  const askAi = async (d: QuestionDraft) => {
    if (!requireAnnotator()) return;
    setBusyKey(d.key);
    try {
      // 替問題找答案的建議要掛在存過的問題上，所以先存
      let target = d;
      if (d.id === null) {
        const q = await api.createQuestion(articleId, {
          text: d.text,
          is_impossible: false,
          reference_answer: d.reference,
          answers: d.answers.map((a) => ({ start: a.start, text: a.text })),
          annotator_id: me,
        });
        replace(d.key, q);
        target = fromQuestion(q);
      }
      const res = await api.suggestAnswer(articleId, target.text, me);
      setSuggestions((list) => [...list, ...res.suggestions]);
      if (res.suggestions.length === 0) toast(res.dropped[0]?.reason ?? "AI 找不到能用的答案", "error");
    } catch (e) {
      fail(e);
    } finally {
      setBusyKey(null);
    }
  };

  const accept = async (s: Suggestion, questionId?: number) => {
    if (!requireAnnotator()) return;
    try {
      const q = await api.acceptSuggestion(s.id, { annotator_id: me, question_id: questionId });
      setSuggestions((list) => list.filter((x) => x.id !== s.id));
      const key = questionId !== undefined ? `q-${questionId}` : `q-${q.id}`;
      replace(key, q);
      setHovered(null);
    } catch (e) {
      fail(e);
    }
  };

  const reject = async (s: Suggestion) => {
    try {
      await api.rejectSuggestion(s.id);
      setSuggestions((list) => list.filter((x) => x.id !== s.id));
      setHovered(null);
    } catch (e) {
      fail(e);
    }
  };

  const edit = (s: Suggestion) => {
    const d = fromSuggestion(s, me);
    setDrafts((list) => [...list, d]);
    setActiveKey(d.key);
    setSuggestions((list) => list.filter((x) => x.id !== s.id));
    setHovered(null);
    window.setTimeout(() => inputs.current.get(d.key)?.focus(), 0);
  };

  const go = useCallback(
    async (where: "prev" | "next" | "random") => {
      if (drafts.some(hasUnsaved) && !window.confirm("有還沒儲存的問題，確定要換文章？")) return;
      let id: number | null | undefined;
      if (where === "prev") id = detail?.prev_id;
      else if (where === "next") id = detail?.next_id;
      else {
        try {
          id = (await api.randomArticle(projectId, { exclude: articleId, unlabeled: unlabeledOnly })).id;
        } catch (e) {
          fail(e);
          return;
        }
      }
      if (id) navigate(`/p/${projectId}/annotate/${id}`);
    },
    [drafts, detail, projectId, articleId, unlabeledOnly],
  );

  useHotkeys([
    { key: "Enter", when: () => pending !== null, run: () => void addPending() },
    { key: "Escape", whileTyping: true, when: () => pending !== null, run: () => setPending(null) },
    { key: "n", run: newQuestion },
    {
      key: "u",
      when: () => active !== null,
      run: () => active && patch(active.key, { isImpossible: !active.isImpossible, dirty: true }),
    },
    { key: "Enter", mod: true, whileTyping: true, when: () => active !== null, run: () => active && void save(active) },
    { key: "g", when: () => aiEnabled, run: () => void generate() },
    { key: "ArrowLeft", run: () => void go("prev") },
    { key: "ArrowRight", run: () => void go("next") },
    { key: "r", run: () => void go("random") },
    { key: "?", run: () => setShowHelp(true) },
  ]);

  if (loadError) return <div className="p-6"><ErrorBanner message={loadError} /></div>;
  if (!detail) return <Loading />;
  const { article } = detail;

  return (
    <div className="flex min-h-screen flex-col xl:flex-row">
      <section className="min-w-0 flex-1 px-4 py-5 sm:px-6 lg:px-8">
        <header className="mb-4 flex flex-wrap items-start gap-3">
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-zinc-500 dark:text-zinc-400">
              文章 {detail.position} / {detail.total}・#{article.id}
            </p>
            <h1 className="truncate text-lg font-semibold tracking-tight">
              {article.title || article.context.slice(0, 40)}
            </h1>
            {article.description && <p className="mt-0.5 text-sm text-zinc-500 dark:text-zinc-400">{article.description}</p>}
          </div>
          <div className="flex flex-wrap items-center gap-1.5">
            <button className="btn btn-secondary px-2" onClick={() => go("prev")} disabled={!detail.prev_id} title="上一篇（←）">
              <ChevronLeft className="size-4" />
            </button>
            <button className="btn btn-secondary" onClick={() => go("random")} title="隨機換一篇（R）">
              <Shuffle className="size-4" />
              隨機
            </button>
            <button className="btn btn-secondary px-2" onClick={() => go("next")} disabled={!detail.next_id} title="下一篇（→）">
              <ChevronRight className="size-4" />
            </button>
            <label className="ml-1 flex items-center gap-1.5 text-xs text-zinc-600 select-none dark:text-zinc-300">
              <input
                type="checkbox"
                className="size-3.5 accent-brand-600"
                checked={unlabeledOnly}
                onChange={(e) => setUnlabeledOnly(e.target.checked)}
              />
              只抽還沒標的
            </label>
            <button className="btn btn-ghost px-2" onClick={() => setShowHelp(true)} title="快捷鍵（?）" aria-label="快捷鍵">
              <Keyboard className="size-4" />
            </button>
          </div>
        </header>

        <div className="card relative p-5 sm:p-6">
          <PassageView
            text={context}
            marks={marks}
            onSelect={(span) => setPending(span)}
            onMarkClick={(ref) => setActiveKey(ref)}
          />
          {pending && (
            <div className="sticky bottom-3 mt-4 flex flex-wrap items-center gap-2 rounded-xl border border-brand-200 bg-white/95 px-3 py-2 text-sm shadow-lg backdrop-blur dark:border-brand-900 dark:bg-zinc-900/95">
              <span className="text-zinc-500 dark:text-zinc-400">已選取</span>
              <span className="max-w-xs truncate font-medium">「{pending.text}」</span>
              <span className="font-mono text-xs text-zinc-500">
                {pending.start}–{pending.end}
              </span>
              <span className="ml-auto flex gap-1.5">
                <button className="btn btn-primary py-1" onClick={() => void addPending()}>
                  加到{active ? ` Q${drafts.indexOf(active) + 1}` : "新問題"} <Kbd>Enter</Kbd>
                </button>
                <button className="btn btn-ghost px-2 py-1" onClick={() => setPending(null)} aria-label="取消選取">
                  <X className="size-4" />
                </button>
              </span>
            </div>
          )}
        </div>

        <div className="mt-5 flex items-center justify-between">
          <h2 className="text-sm font-semibold text-zinc-600 dark:text-zinc-300">問題（{drafts.filter((d) => d.id !== null).length}）</h2>
          <button className="btn btn-secondary" onClick={newQuestion}>
            <Plus className="size-4" /> 新增問題 <Kbd>N</Kbd>
          </button>
        </div>
        <div className="mt-3 flex flex-col gap-3">
          {drafts.map((d, i) => (
            <QuestionCard
              key={d.key}
              ref={(el) => {
                if (el) inputs.current.set(d.key, el);
                else inputs.current.delete(d.key);
              }}
              draft={d}
              index={i}
              active={d.key === activeKey}
              busy={busyKey === d.key}
              aiEnabled={aiEnabled}
              answerSuggestions={forQuestion.get(d.key) ?? []}
              onActivate={() => setActiveKey(d.key)}
              onChange={(p) => patch(d.key, p)}
              onRemoveAnswer={(j) => void removeAnswer(d, j)}
              onSave={() => void save(d)}
              onDelete={() => void remove(d)}
              onAskAi={() => void askAi(d)}
              onAcceptSuggestion={(s) => void accept(s, d.id ?? undefined)}
              onRejectSuggestion={(s) => void reject(s)}
            />
          ))}
        </div>
      </section>

      <div className="border-t border-zinc-200 bg-white/60 px-4 py-5 sm:px-6 xl:sticky xl:top-0 xl:h-screen xl:w-[360px] xl:shrink-0 xl:overflow-y-auto xl:border-t-0 xl:border-l dark:border-zinc-800 dark:bg-zinc-900/40">
        <AiPanel
          suggestions={panel}
          dropped={dropped}
          busy={aiBusy}
          error={aiError}
          count={count}
          onCount={setCount}
          onGenerate={() => void generate()}
          onAccept={(s) => void accept(s)}
          onEdit={edit}
          onReject={(s) => void reject(s)}
          onHover={setHovered}
        />
      </div>

      <Dialog open={showHelp} title="快捷鍵" onClose={() => setShowHelp(false)}>
        <table className="w-full text-sm">
          <tbody>
            {SHORTCUTS.map(([k, v]) => (
              <tr key={k} className="border-b border-zinc-100 last:border-0 dark:border-zinc-800">
                <td className="py-2 pr-4 whitespace-nowrap">
                  <Kbd>{k}</Kbd>
                </td>
                <td className="py-2 text-zinc-600 dark:text-zinc-300">{v}</td>
              </tr>
            ))}
          </tbody>
        </table>
        <p className="mt-3 text-xs text-zinc-500">游標在輸入框裡時，單鍵快捷鍵不會觸發。</p>
      </Dialog>
      <AnnotatorDialog open={askName} onClose={() => setAskName(false)} />
    </div>
  );
}
