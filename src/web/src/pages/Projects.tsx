import { FolderKanban, Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { api } from "../api";
import { PageBody } from "../components/Layout";
import { Dialog, Empty, PageHeader, ProgressBar, useToast } from "../components/ui";
import { useApp } from "../context";
import { percent, ratio } from "../lib/format";
import type { ProjectSummary } from "../types";

function ProjectForm({
  open,
  initial,
  onClose,
}: {
  open: boolean;
  initial: ProjectSummary | null;
  onClose: () => void;
}) {
  const { refreshProjects } = useApp();
  const navigate = useNavigate();
  const [name, setName] = useState(initial?.name ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    try {
      if (initial) await api.updateProject(initial.id, name, description);
      else {
        const p = await api.createProject(name, description);
        navigate(`/p/${p.id}/articles`);
      }
      await refreshProjects();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <Dialog
      open={open}
      title={initial ? "編輯專案" : "新增專案"}
      onClose={onClose}
      footer={
        <>
          <button className="btn btn-secondary" onClick={onClose}>
            取消
          </button>
          <button className="btn btn-primary" onClick={() => void submit()} disabled={!name.trim()}>
            {initial ? "儲存" : "建立"}
          </button>
        </>
      }
    >
      <label className="label" htmlFor="project-name">
        名稱
      </label>
      <input id="project-name" className="input" value={name} onChange={(e) => setName(e.target.value)} />
      <label className="label mt-3" htmlFor="project-desc">
        說明
      </label>
      <textarea
        id="project-desc"
        className="input"
        rows={3}
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="例如：新聞稿問答，給客服機器人評估用"
      />
      {error && <p className="mt-2 text-sm text-rose-600">{error}</p>}
    </Dialog>
  );
}

export function Projects() {
  const { projects, refreshProjects, canAdmin } = useApp();
  const toast = useToast();
  const [editing, setEditing] = useState<ProjectSummary | null | "new">(null);

  const remove = async (p: ProjectSummary) => {
    if (!window.confirm(`刪除專案「${p.name}」？裡面的 ${p.articles} 篇文章和 ${p.questions} 個問題都會一起刪掉。`)) return;
    try {
      await api.deleteProject(p.id);
      await refreshProjects();
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    }
  };

  return (
    <PageBody>
      <PageHeader
        title="所有專案"
        subtitle="一個專案是一組要一起匯出的文章，例如一份資料集。"
        actions={
          canAdmin && (
            <button className="btn btn-primary" onClick={() => setEditing("new")}>
              <Plus className="size-4" /> 新增專案
            </button>
          )
        }
      />
      {projects.length === 0 ? (
        <div className="card">
          <Empty icon={FolderKanban} title="還沒有專案">
            {canAdmin ? "新增一個專案，再匯入文章或 SQuAD 資料集開始標註。" : "請管理員新增專案。"}
          </Empty>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
          {projects.map((p) => (
            <div key={p.id} className="card flex flex-col p-5">
              <div className="flex items-start gap-2">
                <Link to={`/p/${p.id}`} className="min-w-0 flex-1">
                  <h2 className="truncate font-semibold hover:text-brand-700 dark:hover:text-brand-300">{p.name}</h2>
                  <p className="mt-1 line-clamp-2 min-h-10 text-sm text-zinc-500 dark:text-zinc-400">
                    {p.description || "沒有說明"}
                  </p>
                </Link>
                {canAdmin && (
                  <>
                    <button className="btn btn-ghost px-1.5" onClick={() => setEditing(p)} aria-label={`編輯 ${p.name}`}>
                      <Pencil className="size-4" />
                    </button>
                    <button
                      className="btn btn-ghost px-1.5 hover:text-rose-600"
                      onClick={() => void remove(p)}
                      aria-label={`刪除 ${p.name}`}
                    >
                      <Trash2 className="size-4" />
                    </button>
                  </>
                )}
              </div>
              <dl className="mt-4 grid grid-cols-3 gap-2 text-center">
                {[
                  ["文章", p.articles],
                  ["問題", p.questions],
                  ["已通過", p.approved],
                ].map(([k, v]) => (
                  <div key={k} className="rounded-lg bg-zinc-50 py-2 dark:bg-zinc-800/60">
                    <dt className="text-[11px] text-zinc-500 dark:text-zinc-400">{k}</dt>
                    <dd className="text-lg font-semibold tabular-nums">{v}</dd>
                  </div>
                ))}
              </dl>
              <div className="mt-4">
                <div className="mb-1 flex justify-between text-xs text-zinc-500 dark:text-zinc-400">
                  <span>已標註文章</span>
                  <span className="tabular-nums">
                    {p.labeled_articles} / {p.articles}（{percent(ratio(p.labeled_articles, p.articles))}）
                  </span>
                </div>
                <ProgressBar value={ratio(p.labeled_articles, p.articles)} label={`${p.name} 的標註進度`} />
              </div>
              <div className="mt-4 flex gap-2">
                <Link to={`/p/${p.id}/annotate`} className="btn btn-primary flex-1">
                  開始標註
                </Link>
                <Link to={`/p/${p.id}`} className="btn btn-secondary">
                  概覽
                </Link>
              </div>
            </div>
          ))}
        </div>
      )}
      {editing && (
        <ProjectForm open initial={editing === "new" ? null : editing} onClose={() => setEditing(null)} />
      )}
    </PageBody>
  );
}
