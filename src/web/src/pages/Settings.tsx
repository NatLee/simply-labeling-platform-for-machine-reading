import { Sparkles, UserPlus } from "lucide-react";
import { useState } from "react";
import { PageBody } from "../components/Layout";
import { Badge, PageHeader } from "../components/ui";
import { useApp } from "../context";

export function Settings() {
  const { meta, annotators, annotator, setAnnotatorId, addAnnotator } = useApp();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const ai = meta?.ai;

  return (
    <PageBody>
      <PageHeader title="設定" subtitle="標註者和 AI 輔助標注的狀態。" />
      <div className="grid gap-4 lg:grid-cols-2">
        <section className="card p-5">
          <h2 className="mb-1 font-semibold">標註者</h2>
          <p className="mb-4 text-sm text-zinc-500 dark:text-zinc-400">
            不需要登入，只用名字區分誰標了什麼。你選的標註者記在這個瀏覽器裡。
          </p>
          <ul className="mb-4 flex flex-wrap gap-2">
            {annotators.map((a) => (
              <li key={a.id}>
                <button
                  className={`btn ${a.id === annotator?.id ? "btn-primary" : "btn-secondary"}`}
                  onClick={() => setAnnotatorId(a.id)}
                >
                  {a.name}
                </button>
              </li>
            ))}
            {annotators.length === 0 && <li className="text-sm text-zinc-400">還沒有標註者</li>}
          </ul>
          <form
            className="flex gap-2"
            onSubmit={async (e) => {
              e.preventDefault();
              try {
                await addAnnotator(name);
                setName("");
                setError(null);
              } catch (err) {
                setError(err instanceof Error ? err.message : String(err));
              }
            }}
          >
            <input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="新的標註者名字" />
            <button className="btn btn-primary" disabled={!name.trim()}>
              <UserPlus className="size-4" /> 新增
            </button>
          </form>
          {error && <p className="mt-2 text-sm text-rose-600">{error}</p>}
        </section>

        <section className="card p-5">
          <h2 className="mb-1 flex items-center gap-2 font-semibold">
            <Sparkles className="size-4 text-brand-600" /> AI 輔助標注
          </h2>
          {ai?.enabled ? (
            <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
              <dt className="text-zinc-500">狀態</dt>
              <dd>
                <Badge tone="green">開啟</Badge>
              </dd>
              <dt className="text-zinc-500">服務</dt>
              <dd>{ai.provider === "anthropic" ? "Anthropic（Claude API）" : "OpenAI 相容 API"}</dd>
              <dt className="text-zinc-500">模型</dt>
              <dd className="font-mono text-xs leading-5">{ai.model}</dd>
            </dl>
          ) : (
            <>
              <p className="mt-2 text-sm">
                <Badge>關閉</Badge> <span className="ml-1 text-zinc-500 dark:text-zinc-400">{ai?.reason}</span>
              </p>
            </>
          )}
          <p className="mt-4 text-sm text-zinc-500 dark:text-zinc-400">
            AI 的設定都在伺服器的環境變數裡（<code>LLM_PROVIDER</code>、<code>ANTHROPIC_API_KEY</code>、
            <code>OPENAI_BASE_URL</code>…，見 README），改完重新啟動才會生效。API key 不會存進資料庫，也不會傳到瀏覽器。
          </p>
        </section>
      </div>
    </PageBody>
  );
}
