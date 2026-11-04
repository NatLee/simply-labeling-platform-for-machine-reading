import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// jsdom 沒有排版，不提供 scrollIntoView；審核頁換題時會呼叫它。
Element.prototype.scrollIntoView = function scrollIntoView() {};

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});
