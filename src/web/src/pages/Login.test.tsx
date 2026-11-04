import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { App } from "../App";
import { ToastProvider } from "../components/ui";
import { AppProvider } from "../context";
import { fakeApi } from "../test/fakeApi";
import type { AuthUser, Meta, ProjectStats } from "../types";

const NOW = "2026-11-04T00:00:00Z";
const META: Meta = {
  version: "2.1.0",
  ai: { enabled: false, reason: "沒有設定 LLM_PROVIDER" },
  export_formats: ["squad-v2"],
  max_upload_mb: 200,
  auth: { enabled: true },
};
const PROJECT = { id: 1, name: "示範", description: "", created_at: NOW, articles: 0, labeled_articles: 0, questions: 0, approved: 0 };
const STATS: ProjectStats = {
  articles: 0, labeled_articles: 0, questions: 0, pending: 0, approved: 0, rejected: 0, unanswerable: 0,
  needs_answer: 0, answers: 0, multi_annotated: 0, agreement_exact: null, agreement_f1: null,
  suggestions_open: 0, suggestions_accepted: 0, suggestions_rejected: 0, annotators: [],
};

function renderApp(path = "/") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <ToastProvider>
        <AppProvider>
          <App />
        </AppProvider>
      </ToastProvider>
    </MemoryRouter>,
  );
}

describe("built-in login", () => {
  it("shows the login page, signs in, and signs out again", async () => {
    let user: AuthUser | null = null;
    const admin: AuthUser = { id: 1, username: "admin", role: "admin", annotator_id: 7 };
    const calls = fakeApi({
      "GET /meta": () => META,
      "GET /auth/state": () => ({ enabled: true, user }),
      "POST /auth/login": () => {
        user = admin;
        return { user };
      },
      "POST /auth/logout": () => {
        user = null;
        return undefined;
      },
      "GET /annotators": () => [{ id: 7, name: "admin", created_at: NOW }],
      "GET /projects": () => [PROJECT],
    });
    const u = userEvent.setup();
    renderApp();

    await u.type(await screen.findByLabelText("帳號"), "admin");
    await u.type(screen.getByLabelText("密碼"), "admin-password-1");
    await u.click(screen.getByRole("button", { name: "登入" }));
    expect(calls.find((c) => c.path === "/auth/login")!.body).toEqual({ username: "admin", password: "admin-password-1" });

    // 登入後：側欄顯示帳號和角色，沒有「選擇標註者」
    expect(await screen.findAllByText("示範")).not.toHaveLength(0);
    expect(screen.getAllByText("admin").length).toBeGreaterThan(0);
    expect(screen.getAllByText(/管理員/).length).toBeGreaterThan(0);
    expect(screen.queryByText("選擇標註者")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /新增專案/ })).toBeInTheDocument();

    await u.click(screen.getAllByTitle("登出")[0]);
    expect(await screen.findByRole("button", { name: "登入" })).toBeInTheDocument();
    expect(calls.some((c) => c.method === "POST" && c.path === "/auth/logout")).toBe(true);
  });

  it("shows the server's message when the password is wrong", async () => {
    fakeApi({
      "GET /meta": () => META,
      "GET /auth/state": () => ({ enabled: true, user: null }),
    });
    // 沒有 POST /auth/login 的假資料：回 404 和錯誤訊息，畫面要顯示出來
    const u = userEvent.setup();
    renderApp();
    await u.type(await screen.findByLabelText("帳號"), "admin");
    await u.type(screen.getByLabelText("密碼"), "wrong-password");
    await u.click(screen.getByRole("button", { name: "登入" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("no fake for POST /auth/login");
  });

  it("hides review, import/export and project management from annotators", async () => {
    fakeApi({
      "GET /meta": () => META,
      "GET /auth/state": () => ({ enabled: true, user: { id: 2, username: "小明", role: "annotator", annotator_id: 8 } }),
      "GET /annotators": () => [{ id: 8, name: "小明", created_at: NOW }],
      "GET /projects": () => [PROJECT],
      "GET /projects/1/stats": () => STATS,
    });
    renderApp("/p/1");
    expect(await screen.findAllByText("標註")).not.toHaveLength(0);
    await waitFor(() => expect(screen.getAllByText("標註者").length).toBeGreaterThan(0));
    expect(screen.queryByRole("link", { name: "審核" })).not.toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "匯入／匯出" })).not.toBeInTheDocument();
  });

  it("works without login when the server has it off (no change from before)", async () => {
    fakeApi({
      "GET /meta": () => ({ ...META, auth: { enabled: false } }),
      "GET /auth/state": () => ({ enabled: false, user: null }),
      "GET /annotators": () => [],
      "GET /projects": () => [PROJECT],
    });
    renderApp();
    expect(await screen.findAllByText("示範")).not.toHaveLength(0);
    expect(screen.getAllByText("選擇標註者").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "登入" })).not.toBeInTheDocument();
  });
});
