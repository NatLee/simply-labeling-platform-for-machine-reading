import { LogIn } from "lucide-react";
import { useState } from "react";
import { ErrorBanner, Spinner } from "../components/ui";
import { useApp } from "../context";

/** 啟用內建登入、又還沒登入時，整個畫面就是這一頁。 */
export function Login() {
  const { login, meta } = useApp();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await login(username.trim(), password);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPassword("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid min-h-screen place-items-center bg-[radial-gradient(600px_320px_at_80%_0%,var(--color-brand-100),transparent_70%)] px-4 dark:bg-[radial-gradient(600px_320px_at_80%_0%,color-mix(in_oklab,var(--color-brand-500)_20%,transparent),transparent_70%)]">
      <form
        className="card w-full max-w-sm p-6"
        onSubmit={(e) => {
          e.preventDefault();
          void submit();
        }}
      >
        <div className="mb-6 flex items-center gap-3">
          <img src="/favicon.svg" alt="" className="size-10" />
          <div>
            <h1 className="font-semibold tracking-tight">MRC 標注平台</h1>
            <p className="text-xs text-zinc-500 dark:text-zinc-400">登入後開始標註{meta ? `・v${meta.version}` : ""}</p>
          </div>
        </div>
        <label className="label" htmlFor="login-username">
          帳號
        </label>
        <input
          id="login-username"
          className="input"
          autoComplete="username"
          autoFocus
          value={username}
          onChange={(e) => setUsername(e.target.value)}
        />
        <label className="label mt-3" htmlFor="login-password">
          密碼
        </label>
        <input
          id="login-password"
          className="input"
          type="password"
          autoComplete="current-password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        {error && (
          <div className="mt-4">
            <ErrorBanner message={error} />
          </div>
        )}
        <button className="btn btn-primary mt-5 w-full py-2" type="submit" disabled={busy || !username.trim() || !password}>
          {busy ? <Spinner /> : <LogIn className="size-4" />}
          登入
        </button>
        <p className="mt-4 text-xs text-zinc-500 dark:text-zinc-400">忘記密碼請找管理員重設。管理員帳號的密碼是伺服器的 ADMIN_PASSWORD。</p>
      </form>
    </div>
  );
}
