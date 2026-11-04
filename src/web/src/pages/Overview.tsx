import { ArrowDownUp, ClipboardCheck, Highlighter } from "lucide-react";
import { Link, useParams } from "react-router";
import { api } from "../api";
import { PageBody } from "../components/Layout";
import { ErrorBanner, Loading, PageHeader, ProgressBar } from "../components/ui";
import { useApp } from "../context";
import { percent, ratio } from "../lib/format";
import { useAsync } from "../lib/hooks";

function Stat({ label, value, hint }: { label: string; value: string | number; hint?: string }) {
  return (
    <div className="card p-4">
      <p className="text-xs font-medium text-zinc-500 dark:text-zinc-400">{label}</p>
      <p className="mt-1 text-2xl font-semibold tracking-tight tabular-nums">{value}</p>
      {hint && <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">{hint}</p>}
    </div>
  );
}

export function Overview() {
  const projectId = Number(useParams().pid);
  const { projects, canReview } = useApp();
  const project = projects.find((p) => p.id === projectId);
  const { data: s, error } = useAsync(() => api.stats(projectId), [projectId]);

  if (error) return <PageBody><ErrorBanner message={error} /></PageBody>;
  if (!s) return <PageBody><Loading /></PageBody>;

  const decided = s.suggestions_accepted + s.suggestions_rejected;
  return (
    <PageBody>
      <PageHeader
        title={project?.name ?? "專案概覽"}
        subtitle={project?.description || "標註進度、審核狀態和標註者之間的一致程度。"}
        actions={
          <>
            {canReview && (
              <>
                <Link to={`/p/${projectId}/data`} className="btn btn-secondary">
                  <ArrowDownUp className="size-4" /> 匯入／匯出
                </Link>
                <Link to={`/p/${projectId}/review`} className="btn btn-secondary">
                  <ClipboardCheck className="size-4" /> 審核
                </Link>
              </>
            )}
            <Link to={`/p/${projectId}/annotate`} className="btn btn-primary">
              <Highlighter className="size-4" /> 開始標註
            </Link>
          </>
        }
      />

      <div className="card mb-4 p-5">
        <div className="mb-2 flex flex-wrap justify-between gap-2 text-sm">
          <span className="font-medium">標註進度</span>
          <span className="text-zinc-500 tabular-nums dark:text-zinc-400">
            {s.labeled_articles} / {s.articles} 篇文章有問題（{percent(ratio(s.labeled_articles, s.articles))}）
          </span>
        </div>
        <ProgressBar value={ratio(s.labeled_articles, s.articles)} label="標註進度" />
        <div className="mt-4 mb-2 flex flex-wrap justify-between gap-2 text-sm">
          <span className="font-medium">審核進度</span>
          <span className="text-zinc-500 tabular-nums dark:text-zinc-400">
            {s.approved + s.rejected} / {s.questions} 題已審核（通過 {s.approved}、退回 {s.rejected}）
          </span>
        </div>
        <ProgressBar value={ratio(s.approved + s.rejected, s.questions)} label="審核進度" />
      </div>

      <div className="grid grid-cols-2 gap-4 md:grid-cols-3">
        <Stat label="問題" value={s.questions} hint={`${s.answers} 個答案`} />
        <Stat label="待審核" value={s.pending} />
        <Stat label="無法回答" value={s.unanswerable} hint="SQuAD 2.0" />
        <Stat label="還沒有答案" value={s.needs_answer} hint="匯出時會略過" />
        <Stat
          label="標註者一致性（F1）"
          value={percent(s.agreement_f1)}
          hint={s.multi_annotated ? `${s.multi_annotated} 題有多人作答，完全一致 ${percent(s.agreement_exact)}` : "還沒有多人作答的題目"}
        />
        <Stat
          label="AI 建議接受率"
          value={decided ? percent(ratio(s.suggestions_accepted, decided)) : "—"}
          hint={`接受 ${s.suggestions_accepted}、拒絕 ${s.suggestions_rejected}、待處理 ${s.suggestions_open}`}
        />
      </div>

      <div className="card mt-4 overflow-x-auto">
        <div className="border-b border-zinc-200 px-4 py-3 text-sm font-medium dark:border-zinc-800">標註者</div>
        {s.annotators.length === 0 ? (
          <p className="px-4 py-6 text-sm text-zinc-500 dark:text-zinc-400">還沒有人在這個專案標註。</p>
        ) : (
          <table className="w-full">
            <thead>
              <tr>
                <th className="th">名字</th>
                <th className="th text-right">建立的問題</th>
                <th className="th text-right">標的答案</th>
                <th className="th text-right">審核</th>
              </tr>
            </thead>
            <tbody>
              {s.annotators.map((a) => (
                <tr key={a.id} className="border-t border-zinc-100 dark:border-zinc-800">
                  <td className="td font-medium">{a.name}</td>
                  <td className="td text-right tabular-nums">{a.questions}</td>
                  <td className="td text-right tabular-nums">{a.answers}</td>
                  <td className="td text-right tabular-nums">{a.reviews}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </PageBody>
  );
}
