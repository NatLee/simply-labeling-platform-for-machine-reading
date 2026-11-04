import { KeyRound, Sparkles, UserPlus } from "lucide-react";
import { useState } from "react";
import { api } from "../api";
import { PageBody, ROLE_LABEL } from "../components/Layout";
import { Badge, Dialog, ErrorBanner, Loading, PageHeader, useToast } from "../components/ui";
import { useApp } from "../context";
import { useAsync } from "../lib/hooks";
import type { Role, UserAccount } from "../types";

const ROLES: Role[] = ["annotator", "reviewer", "admin"];
const ROLE_HINT: Record<Role, string> = {
  annotator: "標註",
  reviewer: "標註、審核、匯出",
  admin: "全部，包括專案、匯入和帳號",
};

/** 沒啟用登入時：用名字區分標註者（和舊版一樣不需要登入）。 */
function AnnotatorSection() {
  const { annotators, annotator, setAnnotatorId, addAnnotator } = useApp();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  return (
    <section className="card p-5">
      <h2 className="mb-1 font-semibold">標註者</h2>
      <p className="mb-4 text-sm text-zinc-500 dark:text-zinc-400">
        沒有啟用登入，只用名字區分誰標了什麼。你選的標註者記在這個瀏覽器裡。要開啟登入，見 README 的「登入」。
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
  );
}

/** 啟用登入時、管理員看到的帳號管理：新增、改角色、停用、重設密碼。 */
function AccountsSection() {
  const { auth } = useApp();
  const toast = useToast();
  const { data, error, loading, reload, setData } = useAsync(() => api.users(), []);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState<Role>("annotator");
  const [formError, setFormError] = useState<string | null>(null);
  const [resetFor, setResetFor] = useState<UserAccount | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const me = auth?.user?.id;

  const replace = (u: UserAccount) => setData((list) => list?.map((x) => (x.id === u.id ? u : x)));
  const update = async (u: UserAccount, input: Parameters<typeof api.updateUser>[1], done: string) => {
    try {
      replace(await api.updateUser(u.id, input));
      toast(done);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    }
  };

  return (
    <section className="card p-5 lg:col-span-2">
      <h2 className="mb-1 flex items-center gap-2 font-semibold">
        <KeyRound className="size-4 text-brand-600" /> 帳號
      </h2>
      <p className="mb-4 text-sm text-zinc-500 dark:text-zinc-400">
        每個帳號就是一位標註者，問題和答案會記下是誰標的。帳號不能刪除，只能停用；停用和重設密碼會登出那個人所有的裝置。
      </p>
      {error && <ErrorBanner message={error} />}
      {loading && !data ? (
        <Loading />
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-b border-zinc-200 dark:border-zinc-800">
                <th className="th">帳號</th>
                <th className="th">角色</th>
                <th className="th">狀態</th>
                <th className="th text-right">操作</th>
              </tr>
            </thead>
            <tbody>
              {data?.map((u) => (
                <tr key={u.id} className="border-b border-zinc-100 last:border-0 dark:border-zinc-800">
                  <td className="td font-medium">
                    {u.username} {u.id === me && <span className="text-xs text-zinc-400">（你）</span>}
                  </td>
                  <td className="td">
                    <select
                      className="input w-auto py-1"
                      value={u.role}
                      disabled={u.id === me}
                      onChange={(e) => void update(u, { role: e.target.value as Role }, "已改角色")}
                      aria-label={`${u.username} 的角色`}
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {ROLE_LABEL[r]}
                        </option>
                      ))}
                    </select>
                  </td>
                  <td className="td">{u.disabled ? <Badge tone="red">已停用</Badge> : <Badge tone="green">啟用中</Badge>}</td>
                  <td className="td text-right whitespace-nowrap">
                    <button
                      className="btn btn-ghost px-2 py-1 text-xs"
                      onClick={() => {
                        setNewPassword("");
                        setResetFor(u);
                      }}
                    >
                      重設密碼
                    </button>
                    {u.id !== me && (
                      <button
                        className="btn btn-secondary px-2 py-1 text-xs"
                        onClick={() =>
                          void update(u, { disabled: !u.disabled }, u.disabled ? "已啟用" : "已停用")
                        }
                      >
                        {u.disabled ? "啟用" : "停用"}
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <form
        className="mt-4 grid gap-2 sm:grid-cols-[1fr_1fr_auto_auto]"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await api.createUser(username.trim(), password, role);
            setUsername("");
            setPassword("");
            setFormError(null);
            toast("已新增帳號");
            await reload();
          } catch (err) {
            setFormError(err instanceof Error ? err.message : String(err));
          }
        }}
      >
        <input className="input" value={username} onChange={(e) => setUsername(e.target.value)} placeholder="帳號" aria-label="新帳號" />
        <input
          className="input"
          type="password"
          autoComplete="new-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          placeholder="密碼（至少 8 個字元）"
          aria-label="新帳號的密碼"
        />
        <select className="input w-auto" value={role} onChange={(e) => setRole(e.target.value as Role)} aria-label="新帳號的角色">
          {ROLES.map((r) => (
            <option key={r} value={r}>
              {ROLE_LABEL[r]}（{ROLE_HINT[r]}）
            </option>
          ))}
        </select>
        <button className="btn btn-primary" disabled={!username.trim() || password.length < 8}>
          <UserPlus className="size-4" /> 新增帳號
        </button>
      </form>
      {formError && <p className="mt-2 text-sm text-rose-600">{formError}</p>}
      <Dialog
        open={resetFor !== null}
        title={`重設 ${resetFor?.username ?? ""} 的密碼`}
        onClose={() => setResetFor(null)}
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setResetFor(null)}>
              取消
            </button>
            <button
              className="btn btn-primary"
              disabled={newPassword.length < 8}
              onClick={() => {
                if (resetFor) void update(resetFor, { password: newPassword }, "已重設密碼");
                setResetFor(null);
              }}
            >
              重設
            </button>
          </>
        }
      >
        <input
          className="input"
          type="password"
          autoComplete="new-password"
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
          placeholder="新密碼（至少 8 個字元）"
          aria-label="新密碼"
        />
        <p className="hint">重設後，這個帳號所有已登入的裝置都會被登出。</p>
      </Dialog>
    </section>
  );
}

/** 啟用登入時、不是管理員的人看到自己的帳號。 */
function MyAccount() {
  const { auth } = useApp();
  const user = auth?.user;
  if (!user) return null;
  return (
    <section className="card p-5">
      <h2 className="mb-1 font-semibold">你的帳號</h2>
      <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
        <dt className="text-zinc-500">帳號</dt>
        <dd>{user.username}</dd>
        <dt className="text-zinc-500">角色</dt>
        <dd>
          {ROLE_LABEL[user.role]}（{ROLE_HINT[user.role]}）
        </dd>
      </dl>
      <p className="mt-4 text-sm text-zinc-500 dark:text-zinc-400">要改密碼或角色，請找管理員。</p>
    </section>
  );
}

export function Settings() {
  const { meta, auth, canAdmin } = useApp();
  const ai = meta?.ai;
  const loginOn = Boolean(auth?.enabled);

  return (
    <PageBody>
      <PageHeader title="設定" subtitle={loginOn ? "帳號和 AI 輔助標注的狀態。" : "標註者和 AI 輔助標注的狀態。"} />
      <div className="grid gap-4 lg:grid-cols-2">
        {!loginOn ? <AnnotatorSection /> : canAdmin ? <AccountsSection /> : <MyAccount />}

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
            <p className="mt-2 text-sm">
              <Badge>關閉</Badge> <span className="ml-1 text-zinc-500 dark:text-zinc-400">{ai?.reason}</span>
            </p>
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
