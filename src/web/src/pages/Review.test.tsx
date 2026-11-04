import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { fakeApi } from "../test/fakeApi";
import { renderAt } from "../test/render";
import type { ReviewItem, ReviewPage } from "../types";
import { Review } from "./Review";

const NOW = "2026-11-04T00:00:00Z";
const CONTEXT = "台北 101 高 508 公尺。";

const ITEM: ReviewItem = {
  id: 7,
  article_id: 3,
  article_title: "台北 101",
  text: "台北 101 有多高？",
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
  answers: [
    { id: 1, question_id: 7, start: 9, text: "508 公尺", annotator_id: 1, origin: "human", created_at: NOW },
    { id: 2, question_id: 7, start: 9, text: "508", annotator_id: 2, origin: "human", created_at: NOW },
  ],
  agreement: { annotators: 2, exact: 0, f1: 0.5 },
};

describe("review page", () => {
  it("shows agreement and approves with the A key", async () => {
    const page: ReviewPage = { items: [ITEM], contexts: { "3": CONTEXT }, total: 1 };
    const calls = fakeApi({
      "GET /meta": () => null,
      "GET /annotators": () => [
        { id: 1, name: "小明", created_at: NOW },
        { id: 2, name: "小華", created_at: NOW },
      ],
      "GET /projects": () => [],
      "GET /projects/1/review": () => page,
      "POST /questions/7/review": () => ({ ...ITEM, status: "approved", reviewed_by: 2 }),
    });
    const user = userEvent.setup();
    renderAt("/p/1/review", "/p/:pid/review", <Review />, { annotatorId: 2 });

    expect(await screen.findByText("台北 101 有多高？")).toBeInTheDocument();
    expect(screen.getByText("2 人・F1 50%")).toBeInTheDocument();
    expect(screen.getByText("· 小華")).toBeInTheDocument();

    await user.keyboard("a");
    await waitFor(() => expect(calls.some((c) => c.path === "/questions/7/review")).toBe(true));
    expect(calls.find((c) => c.path === "/questions/7/review")!.body).toEqual({
      status: "approved",
      annotator_id: 2,
      note: "",
    });
    // 在「待審核」分頁，通過的題目會從列表拿掉
    await waitFor(() => expect(screen.queryByText("台北 101 有多高？")).not.toBeInTheDocument());
  });
});
