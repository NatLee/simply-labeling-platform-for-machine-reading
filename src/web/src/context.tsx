import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api } from "./api";
import { useStoredState } from "./lib/hooks";
import type { Annotator, Meta, ProjectSummary } from "./types";

interface AppState {
  meta: Meta | null;
  annotators: Annotator[];
  /** 目前的標註者（存在瀏覽器裡）。還沒選時是 null，寫入的操作會先請使用者選。 */
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
  const [annotators, setAnnotators] = useState<Annotator[]>([]);
  const [annotatorId, setAnnotatorId] = useStoredState<number | null>(ANNOTATOR_KEY, null);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);

  const refreshProjects = useCallback(async () => {
    setProjects(await api.projects());
  }, []);

  useEffect(() => {
    api.meta().then(setMeta).catch(() => setMeta(null));
    api.annotators().then(setAnnotators).catch(() => setAnnotators([]));
    refreshProjects().catch(() => setProjects([]));
  }, [refreshProjects]);

  const addAnnotator = useCallback(
    async (name: string) => {
      const a = await api.createAnnotator(name);
      setAnnotators((list) => (list.some((x) => x.id === a.id) ? list : [...list, a].sort((x, y) => x.name.localeCompare(y.name))));
      setAnnotatorId(a.id);
      return a;
    },
    [setAnnotatorId],
  );

  const value = useMemo<AppState>(() => {
    const byId = new Map(annotators.map((a) => [a.id, a]));
    return {
      meta,
      annotators,
      annotator: annotatorId === null ? null : (byId.get(annotatorId) ?? null),
      setAnnotatorId,
      addAnnotator,
      projects,
      refreshProjects,
      annotatorName: (id) => (id === null ? "匿名" : (byId.get(id)?.name ?? `#${id}`)),
    };
  }, [meta, annotators, annotatorId, setAnnotatorId, addAnnotator, projects, refreshProjects]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useApp(): AppState {
  const v = useContext(Ctx);
  if (!v) throw new Error("useApp must be used inside <AppProvider>");
  return v;
}
