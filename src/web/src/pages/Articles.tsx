import { FileText, Plus, Search, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api, type ArticleFilter } from "../api";
import { PageBody } from "../components/Layout";
import { Badge, Dialog, Empty, ErrorBanner, Loading, PageHeader, useToast } from "../components/ui";
import { useApp } from "../context";
import { useAsync } from "../lib/hooks";

const PAGE = 50;
const FILTERS: { value: ArticleFilter | undefined; label: string }[] = [
  { value: undefined, label: "全部" },
  { value: "unlabeled", label: "還沒標註" },
  { value: "pending", label: "待審核" },
  { value: "reviewed", label: "已審核" },
];

/** 新增一篇文章（舊版「Insert an article」：文章內容和說明）。 */
function NewArticle({ projectId, open, onClose }: { projectId: number; open: boolean; onClose: () => void }) {
  const navigate = useNavigate();
  const toast = useToast();
  const { refreshProjects } = useApp();
  const [title, setTitle] = useState("");
  const [context, setContext] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);
  const submit = async (thenAnnotate: boolean) => {
    try {
      const res = await api.createArticle(projectId, { title, context, description });
      toast(res.created ? "文章已新增" : "專案裡已經有一樣的文章，幫你打開它");
      await refreshProjects();
      setTitle("");
      setContext("");
      setDescription("");
      onClose();
      if (thenAnnotate || !res.created) navigate(`/p/${projectId}/annotate/${res.id}`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  return (
    <Dialog
      open={open}
      title="新增文章"
      onClose={onClose}
      wide
      footer={
        <>
          <button className="btn btn-secondary" onClick={() => void submit(false)} disabled={!context.trim()}>
            新增
          </button>
          <button className="btn btn-primary" onClick={() => void submit(true)} disabled={!context.trim()}>
            新增並開始標註
          </button>
        </>
      }
    >
      <label className="label" htmlFor="a-title">
        標題 <span className="font-normal text-zinc-400">（選填，匯出 SQuAD 時當 title）</span>
      </label>
      <input id="a-title" className="input" value={title} onChange={(e) => setTitle(e.target.value)} />
      <label className="label mt-3" htmlFor="a-context">
        文章內容
      </label>
      <textarea
        id="a-context"
        className="input font-normal leading-7"
        rows={10}
        value={context}
        onChange={(e) => setContext(e.target.value)}
        placeholder="貼上要標註的文章"
      />
      <p className="hint">文章存了之後就不能改內容，因為答案的位置是照原文算的。</p>
      <label className="label mt-3" htmlFor="a-desc">
        說明 <span className="font-normal text-zinc-400">（選填，例如出處）</span>
      </label>
      <textarea
        id="a-desc"
        className="input"
        rows={2}
        value={description}
        onChange={(e) => setDescription(e.target.value)}
      />
      {error && <p className="mt-2 text-sm text-rose-600">{error}</p>}
    </Dialog>
  );
}

export function Articles() {
  const projectId = Number(useParams().pid);
  const toast = useToast();
  const { refreshProjects, canAdmin } = useApp();
  const [q, setQ] = useState("");
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<ArticleFilter | undefined>(undefined);
  const [offset, setOffset] = useState(0);
  const [adding, setAdding] = useState(false);

  useEffect(() => {
    const t = window.setTimeout(() => {
      setSearch(q);
      setOffset(0);
    }, 250);
    return () => window.clearTimeout(t);
  }, [q]);

  const { data, error, loading, reload } = useAsync(
    () => api.articles(projectId, { q: search, filter, limit: PAGE, offset }),
    [projectId, search, filter, offset],
  );

  const remove = async (id: number, label: string) => {
    if (!window.confirm(`刪除文章「${label}」和它所有的問題？`)) return;
    try {
      await api.deleteArticle(id);
      await Promise.all([reload(), refreshProjects()]);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    }
  };

  return (
    <PageBody>
      <PageHeader
        title="文章"
        subtitle="新增、搜尋和管理要標註的文章。大量匯入請用「匯入／匯出」。"
        actions={
          <button className="btn btn-primary" onClick={() => setAdding(true)}>
            <Plus className="size-4" /> 新增文章
          </button>
        }
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <label className="relative min-w-60 flex-1">
          <Search className="pointer-events-none absolute top-2.5 left-3 size-4 text-zinc-400" />
          <input
            className="input pl-9"
            placeholder="搜尋標題、內容或說明"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            aria-label="搜尋文章"
          />
        </label>
        <div className="flex rounded-lg bg-zinc-100 p-0.5 dark:bg-zinc-800">
          {FILTERS.map((f) => (
            <button
              key={f.label}
              onClick={() => {
                setFilter(f.value);
                setOffset(0);
              }}
              className={`rounded-md px-3 py-1.5 text-sm ${
                filter === f.value
                  ? "bg-white font-medium text-brand-700 shadow-sm dark:bg-zinc-950 dark:text-brand-300"
                  : "text-zinc-500 dark:text-zinc-400"
              }`}
            >
              {f.label}
            </button>
          ))}
        </div>
      </div>
      {error && <ErrorBanner message={error} />}
      {loading && !data ? (
        <Loading />
      ) : data && data.items.length === 0 ? (
        <div className="card">
          <Empty icon={FileText} title={search || filter ? "沒有符合的文章" : "還沒有文章"}>
            {!search && !filter && "按「新增文章」貼上一篇，或到「匯入／匯出」匯入 SQuAD、JSONL 或舊版資料庫。"}
          </Empty>
        </div>
      ) : (
        data && (
          <div className="card overflow-x-auto">
            <table className="w-full">
              <thead>
                <tr className="border-b border-zinc-200 dark:border-zinc-800">
                  <th className="th w-16">#</th>
                  <th className="th">文章</th>
                  <th className="th w-20 text-right">字數</th>
                  <th className="th w-40">問題</th>
                  <th className="th w-10" />
                </tr>
              </thead>
              <tbody>
                {data.items.map((a) => {
                  const label = a.title || a.preview.slice(0, 30);
                  return (
                    <tr key={a.id} className="border-b border-zinc-100 last:border-0 hover:bg-zinc-50 dark:border-zinc-800 dark:hover:bg-zinc-800/40">
                      <td className="td font-mono text-xs text-zinc-500">{a.id}</td>
                      <td className="td max-w-0">
                        <Link to={`/p/${projectId}/annotate/${a.id}`} className="block">
                          {a.title && <span className="block truncate font-medium">{a.title}</span>}
                          <span className="block truncate text-zinc-500 dark:text-zinc-400">{a.preview}</span>
                          {a.description && (
                            <span className="block truncate text-xs text-zinc-400">{a.description}</span>
                          )}
                        </Link>
                      </td>
                      <td className="td text-right text-zinc-500 tabular-nums">{a.length}</td>
                      <td className="td">
                        <div className="flex flex-wrap gap-1">
                          {a.questions === 0 ? (
                            <Badge>還沒標註</Badge>
                          ) : (
                            <>
                              <Badge tone="brand">{a.questions} 題</Badge>
                              {a.pending > 0 && <Badge tone="amber">{a.pending} 待審</Badge>}
                              {a.approved > 0 && <Badge tone="green">{a.approved} 通過</Badge>}
                            </>
                          )}
                        </div>
                      </td>
                      <td className="td">
                        {canAdmin && (
                          <button
                            className="btn btn-ghost px-1.5 py-1 hover:text-rose-600"
                            onClick={() => void remove(a.id, label)}
                            aria-label={`刪除 ${label}`}
                          >
                            <Trash2 className="size-4" />
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
            <div className="flex items-center justify-between border-t border-zinc-200 px-4 py-2.5 text-sm text-zinc-500 dark:border-zinc-800">
              <span className="tabular-nums">
                {offset + 1}–{offset + data.items.length}，共 {data.total} 篇
              </span>
              <span className="flex gap-2">
                <button className="btn btn-secondary py-1" disabled={offset === 0} onClick={() => setOffset(Math.max(0, offset - PAGE))}>
                  上一頁
                </button>
                <button
                  className="btn btn-secondary py-1"
                  disabled={offset + PAGE >= data.total}
                  onClick={() => setOffset(offset + PAGE)}
                >
                  下一頁
                </button>
              </span>
            </div>
          </div>
        )
      )}
      <NewArticle projectId={projectId} open={adding} onClose={() => setAdding(false)} />
    </PageBody>
  );
}
