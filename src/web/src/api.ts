import type {
  Annotator,
  AuthState,
  AuthUser,
  Article,
  ArticleDetail,
  ArticleListItem,
  ImportSummary,
  Meta,
  Page,
  Project,
  ProjectStats,
  ProjectSummary,
  Question,
  ReviewPage,
  ReviewStatus,
  Role,
  SpanInput,
  SuggestResponse,
  Suggestion,
  UserAccount,
} from "./types";

/** 啟用登入時，session 過期或沒登入會收到 401；App 聽到這個事件就回到登入頁。 */
export const UNAUTHORIZED_EVENT = "mrc:unauthorized";

/** 後端回 `{"error": "訊息"}` 時丟出這個，訊息直接給使用者看。 */
export class ApiError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const init: RequestInit = { method, headers: {} };
  if (body instanceof Blob || body instanceof ArrayBuffer) {
    init.body = body;
    (init.headers as Record<string, string>)["content-type"] = "application/octet-stream";
  } else if (body !== undefined) {
    init.body = JSON.stringify(body);
    (init.headers as Record<string, string>)["content-type"] = "application/json";
  }
  const res = await fetch(`/api${path}`, init);
  if (res.status === 204) return undefined as T;
  const text = await res.text();
  let data: unknown = undefined;
  try {
    data = text ? JSON.parse(text) : undefined;
  } catch {
    data = text;
  }
  if (res.status === 401 && path !== "/auth/login") {
    window.dispatchEvent(new Event(UNAUTHORIZED_EVENT));
  }
  if (!res.ok) {
    const message =
      typeof data === "object" && data !== null && "error" in data
        ? String((data as { error: unknown }).error)
        : `伺服器回應 ${res.status}`;
    throw new ApiError(res.status, message);
  }
  return data as T;
}

const get = <T>(path: string) => request<T>("GET", path);
const post = <T>(path: string, body?: unknown) => request<T>("POST", path, body ?? {});
const put = <T>(path: string, body: unknown) => request<T>("PUT", path, body);
const del = (path: string) => request<void>("DELETE", path);

function query(params: Record<string, string | number | boolean | null | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== null && v !== "") q.set(k, String(v));
  }
  const s = q.toString();
  return s ? `?${s}` : "";
}

export type ArticleFilter = "unlabeled" | "pending" | "reviewed";

export const api = {
  meta: () => get<Meta>("/meta"),

  authState: () => get<AuthState>("/auth/state"),
  login: (username: string, password: string) => post<{ user: AuthUser }>("/auth/login", { username, password }),
  logout: () => post<void>("/auth/logout"),
  users: () => get<UserAccount[]>("/users"),
  createUser: (username: string, password: string, role: Role) =>
    post<UserAccount>("/users", { username, password, role }),
  updateUser: (id: number, input: { role?: Role; disabled?: boolean; password?: string }) =>
    put<UserAccount>(`/users/${id}`, input),

  annotators: () => get<Annotator[]>("/annotators"),
  createAnnotator: (name: string) => post<Annotator>("/annotators", { name }),

  projects: () => get<ProjectSummary[]>("/projects"),
  project: (id: number) => get<Project>(`/projects/${id}`),
  createProject: (name: string, description: string) => post<Project>("/projects", { name, description }),
  updateProject: (id: number, name: string, description: string) =>
    put<Project>(`/projects/${id}`, { name, description }),
  deleteProject: (id: number) => del(`/projects/${id}`),
  stats: (id: number) => get<ProjectStats>(`/projects/${id}/stats`),

  articles: (projectId: number, opts: { q?: string; filter?: ArticleFilter; limit?: number; offset?: number }) =>
    get<Page<ArticleListItem>>(`/projects/${projectId}/articles${query(opts)}`),
  createArticle: (projectId: number, input: { title: string; context: string; description: string }) =>
    post<{ id: number; created: boolean }>(`/projects/${projectId}/articles`, input),
  randomArticle: (projectId: number, opts: { exclude?: number; unlabeled?: boolean }) =>
    get<Article>(`/projects/${projectId}/articles/random${query(opts)}`),
  article: (id: number) => get<ArticleDetail>(`/articles/${id}`),
  updateArticle: (id: number, input: { title?: string; description?: string }) =>
    put<ArticleDetail>(`/articles/${id}`, input),
  deleteArticle: (id: number) => del(`/articles/${id}`),

  createQuestion: (
    articleId: number,
    input: {
      text: string;
      is_impossible: boolean;
      reference_answer: string;
      answers: SpanInput[];
      annotator_id: number | null;
    },
  ) => post<Question>(`/articles/${articleId}/questions`, input),
  question: (id: number) => get<Question>(`/questions/${id}`),
  updateQuestion: (id: number, input: { text?: string; is_impossible?: boolean; reference_answer?: string }) =>
    put<Question>(`/questions/${id}`, input),
  deleteQuestion: (id: number) => del(`/questions/${id}`),
  addAnswer: (questionId: number, span: SpanInput, annotatorId: number | null) =>
    post<Question>(`/questions/${questionId}/answers`, { ...span, annotator_id: annotatorId }),
  deleteAnswer: (id: number) => del(`/answers/${id}`),
  review: (questionId: number, status: ReviewStatus, annotatorId: number | null, note = "") =>
    post<Question>(`/questions/${questionId}/review`, { status, annotator_id: annotatorId, note }),
  reviewQueue: (projectId: number, opts: { status?: ReviewStatus; limit?: number; offset?: number }) =>
    get<ReviewPage>(`/projects/${projectId}/review${query(opts)}`),

  importFile: (projectId: number, file: Blob) => request<ImportSummary>("POST", `/projects/${projectId}/import`, file),
  exportUrl: (projectId: number, format: string, onlyApproved: boolean) =>
    `/api/projects/${projectId}/export${query({ format, only_approved: onlyApproved || undefined })}`,

  suggestions: (articleId: number) => get<Suggestion[]>(`/articles/${articleId}/suggestions`),
  generateSuggestions: (articleId: number, count: number, annotatorId: number | null) =>
    post<SuggestResponse>(`/articles/${articleId}/suggestions`, { count, annotator_id: annotatorId }),
  suggestAnswer: (articleId: number, question: string, annotatorId: number | null) =>
    post<SuggestResponse>(`/articles/${articleId}/suggestions/answer`, { question, annotator_id: annotatorId }),
  acceptSuggestion: (
    id: number,
    input: {
      annotator_id: number | null;
      question?: string;
      is_impossible?: boolean;
      reference_answer?: string;
      answers?: SpanInput[];
      question_id?: number;
    },
  ) => post<Question>(`/suggestions/${id}/accept`, input),
  rejectSuggestion: (id: number) => post<Suggestion>(`/suggestions/${id}/reject`),
};
