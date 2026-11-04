import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeApi } from "../test/fakeApi";
import { renderAt, selectInPassage } from "../test/render";
import type { ArticleDetail, Meta, Question, Suggestion } from "../types";
import { Annotate } from "./Annotate";

const CONTEXT = "台北😀101 高 508 公尺，在 2004 年完工。";
const NOW = "2026-11-04T00:00:00Z";

const META: Meta = {
  version: "2.0.0",
  ai: { enabled: true, provider: "anthropic", model: "claude-opus-5-5" },
  export_formats: ["squad-v2"],
  max_upload_mb: 200,
};

function detail(questions: Question[] = []): ArticleDetail {
  return {
    article: { id: 5, project_id: 1, title: "台北 101", context: CONTEXT, description: "測試用", created_at: NOW },
    questions,
    position: 1,
    total: 3,
    prev_id: null,
    next_id: 6,
  };
}

function question(id: number, text: string, start: number, answer: string): Question {
  return {
    id,
    article_id: 5,
    text,
    is_impossible: false,
    reference_answer: "",
    origin: "human",
    external_id: null,
    created_by: 1,
    status: "pending",
    reviewed_by: null,
    reviewed_at: null,
    review_note: "",
    created_at: NOW,
    updated_at: NOW,
    answers: [{ id: id * 10, question_id: id, start, text: answer, annotator_id: 1, origin: "human", created_at: NOW }],
    agreement: null,
  };
}

const SUGGESTION: Suggestion = {
  id: 9,
  article_id: 5,
  question: "在哪一年完工？",
  is_impossible: false,
  answer_start: 18,
  answer_text: "2004 年",
  reference_answer: "2004 年完工。",
  provider: "anthropic",
  model: "claude-opus-5-5",
  status: "open",
  question_id: null,
  requested_by: 1,
  created_at: NOW,
  decided_at: null,
};

function baseRoutes() {
  return {
    "GET /meta": () => META,
    "GET /annotators": () => [{ id: 1, name: "小明", created_at: NOW }],
    "GET /projects": () => [],
    "GET /articles/5": () => detail(),
    "GET /articles/5/suggestions": () => [],
  };
}

describe("annotate page", () => {
  it("turns a selection into a character-offset answer and saves the question", async () => {
    const calls = fakeApi({
      ...baseRoutes(),
      "POST /articles/5/questions": (body) => {
        const b = body as { text: string; answers: { start: number; text: string }[] };
        return question(31, b.text, b.answers[0].start, b.answers[0].text);
      },
    });
    const user = userEvent.setup();
    renderAt("/p/1/annotate/5", "/p/:pid/annotate/:aid", <Annotate />, { annotatorId: 1 });

    const passage = await screen.findByTestId("passage");
    // 「508 公尺」：UTF-16 位置 10–16；😀 佔兩個單位，所以字元位置是 9–15
    selectInPassage(passage, 10, 16);
    fireEvent.mouseUp(passage);
    expect(await screen.findByText("「508 公尺」")).toBeInTheDocument();
    expect(screen.getByText("9–15")).toBeInTheDocument();

    await user.keyboard("{Enter}");
    await user.type(screen.getByLabelText("問題 1"), "台北 101 有多高？");
    await user.click(screen.getByRole("button", { name: /儲存/ }));

    await waitFor(() => expect(calls.some((c) => c.method === "POST" && c.path === "/articles/5/questions")).toBe(true));
    const post = calls.find((c) => c.path === "/articles/5/questions")!;
    expect(post.body).toEqual({
      text: "台北 101 有多高？",
      is_impossible: false,
      reference_answer: "",
      answers: [{ start: 9, text: "508 公尺" }],
      annotator_id: 1,
    });
    expect(await screen.findByText("已儲存")).toBeInTheDocument();
  });

  it("shows AI suggestions and accepting one creates a question", async () => {
    const calls = fakeApi({
      ...baseRoutes(),
      "GET /articles/5": () => detail([question(31, "台北 101 有多高？", 9, "508 公尺")]),
      "POST /articles/5/suggestions": () => ({
        suggestions: [SUGGESTION],
        dropped: [{ question: "誰設計的？", reason: "答案不是文章裡一字不差的片段" }],
      }),
      "POST /suggestions/9/accept": () => question(32, SUGGESTION.question, 18, "2004 年"),
    });
    const user = userEvent.setup();
    renderAt("/p/1/annotate/5", "/p/:pid/annotate/:aid", <Annotate />, { annotatorId: 1 });

    expect(await screen.findByText("Anthropic・claude-opus-5-5")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /產生建議/ }));
    expect(await screen.findByText("在哪一年完工？")).toBeInTheDocument();
    expect(screen.getByText(/丟掉 1 個沒通過檢查的建議/)).toBeInTheDocument();
    const gen = calls.find((c) => c.path === "/articles/5/suggestions" && c.method === "POST")!;
    expect(gen.body).toEqual({ count: 5, annotator_id: 1 });

    await user.click(screen.getByRole("button", { name: "接受" }));
    await waitFor(() => expect(screen.getByLabelText("問題 2")).toHaveValue("在哪一年完工？"));
    const accept = calls.find((c) => c.path === "/suggestions/9/accept")!;
    expect(accept.body).toEqual({ annotator_id: 1 });
  });

  it("hides other annotators' answers until you answer (blind labeling)", async () => {
    const theirs = { ...question(31, "台北 101 有多高？", 9, "508 公尺"), created_by: 2 };
    theirs.answers = theirs.answers.map((a) => ({ ...a, annotator_id: 2 }));
    fakeApi({ ...baseRoutes(), "GET /articles/5": () => detail([theirs]) });
    const user = userEvent.setup();
    renderAt("/p/1/annotate/5", "/p/:pid/annotate/:aid", <Annotate />, { annotatorId: 1 });

    expect(await screen.findByText(/先標你的答案/)).toBeInTheDocument();
    expect(screen.queryByText("508 公尺")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /顯示其他 1 個答案/ }));
    expect(screen.getAllByText("508 公尺").length).toBeGreaterThan(0);
  });

  it("explains how to turn AI on when it is off", async () => {
    fakeApi({ ...baseRoutes(), "GET /meta": () => ({ ...META, ai: { enabled: false, reason: "沒有設定 LLM_PROVIDER" } }) });
    renderAt("/p/1/annotate/5", "/p/:pid/annotate/:aid", <Annotate />, { annotatorId: 1 });
    expect(await screen.findByText("AI 輔助標注沒有開啟")).toBeInTheDocument();
    expect(screen.getByText("沒有設定 LLM_PROVIDER")).toBeInTheDocument();
  });
});
