import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, UNAUTHORIZED_EVENT } from "./api";
import { useStoredState } from "./lib/hooks";
import type { Annotator, AuthState, AuthUser, Meta, ProjectSummary } from "./types";

interface AppState {
  meta: Meta | null;
  /** 登入狀態；載入前是 null。沒啟用登入時 `enabled` 是 false。 */
  auth: AuthState | null;
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  /** 審核、匯出（沒啟用登入時大家都可以）。 */
  canReview: boolean;
  /** 專案、刪文章、匯入、帳號（沒啟用登入時大家都可以）。 */
  canAdmin: boolean;
  annotators: Annotator[];
  /**
   * 目前的標註者。啟用登入時就是登入的人；沒啟用時是使用者自己選的（存在瀏覽器裡），
   * 還沒選時是 null，寫入的操作會先請使用者選。
   */
  annotator: Annotator | null;
  setAnnotatorId: (id: number | null) => void;
  addAnnotator: (name: string) => Promise<Annotator>;
  projects: ProjectSummary[];
  refreshProjects: () => Promise<void>;
  /** 標註者在統計裡顯示的名字。 */
  annotatorName: (id: number | null) => string;
}

const Ctx = createContext<AppState | null>(null);

export const ANNOTATOR_KEY = "mrc.annotatorId";

export function AppProvider({ children }: { children: ReactNode }) {
  const [meta, setMeta] = useState<Meta | null>(null);
  const [auth, setAuth] = useState<AuthState | null>(null);
  const [annotators, setAnnotators] = useState<Annotator[]>([]);
  const [chosenId, setAnnotatorId] = useStoredState<number | null>(ANNOTATOR_KEY, null);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);

  const refreshProjects = useCallback(async () => {
    setProjects(await api.projects());
  }, []);

  /** 登入狀態確定、而且看得到資料時，載入標註者和專案。 */
  const loadData = useCallback(async () => {
    await Promise.all([
      api.annotators().then(setAnnotators).catch(() => setAnnotators([])),
      refreshProjects().catch(() => setProjects([])),
    ]);
  }, [refreshProjects]);

  const refreshAuth = useCallback(async () => {
    try {
      const state = await api.authState();
      setAuth(state);
      if (!state.enabled || state.user) await loadData();
    } catch {
      // 舊版後端沒有這個 API：當成沒啟用登入
      setAuth({ enabled: false, user: null });
      await loadData();
    }
  }, [loadData]);

  useEffect(() => {
    api.meta().then(setMeta).catch(() => setMeta(null));
    void refreshAuth();
    const onUnauthorized = () => setAuth((a) => (a?.enabled ? { enabled: true, user: null } : a));
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
  }, [refreshAuth]);

  const login = useCallback(
    async (username: string, password: string) => {
      const { user } = await api.login(username, password);
      setAuth({ enabled: true, user });
      await loadData();
    },
    [loadData],
  );

  const logout = useCallback(async () => {
    try {
      await api.logout();
    } finally {
      setAuth((a) => (a ? { ...a, user: null } : a));
      setProjects([]);
    }
  }, []);

  const addAnnotator = useCallback(
    async (name: string) => {
      const a = await api.createAnnotator(name);
      setAnnotators((list) =>
        list.some((x) => x.id === a.id) ? list : [...list, a].sort((x, y) => x.name.localeCompare(y.name)),
      );
      setAnnotatorId(a.id);
      return a;
    },
    [setAnnotatorId],
  );

  const value = useMemo<AppState>(() => {
    const byId = new Map(annotators.map((a) => [a.id, a]));
    const user: AuthUser | null = auth?.enabled ? auth.user : null;
    const annotator = user
      ? (byId.get(user.annotator_id) ?? { id: user.annotator_id, name: user.username, created_at: "" })
      : chosenId === null
        ? null
        : (byId.get(chosenId) ?? null);
    const open = !auth?.enabled;
    return {
      meta,
      auth,
      login,
      logout,
      canReview: open || user?.role === "admin" || user?.role === "reviewer",
      canAdmin: open || user?.role === "admin",
      annotators,
      annotator,
      setAnnotatorId,
      addAnnotator,
      projects,
      refreshProjects,
      annotatorName: (id) => (id === null ? "匿名" : (byId.get(id)?.name ?? `#${id}`)),
    };
  }, [meta, auth, login, logout, annotators, chosenId, setAnnotatorId, addAnnotator, projects, refreshProjects]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useApp(): AppState {
  const v = useContext(Ctx);
  if (!v) throw new Error("useApp must be used inside <AppProvider>");
  return v;
}
