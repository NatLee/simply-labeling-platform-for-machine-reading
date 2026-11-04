import { render } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { ToastProvider } from "../components/ui";
import { ANNOTATOR_KEY, AppProvider } from "../context";
import type { ReactElement } from "react";

/** 用和正式環境一樣的 provider 包起來，從 `path` 開始，`route` 是要比對的路由。 */
export function renderAt(path: string, route: string, element: ReactElement, opts: { annotatorId?: number } = {}) {
  if (opts.annotatorId !== undefined) localStorage.setItem(ANNOTATOR_KEY, JSON.stringify(opts.annotatorId));
  return render(
    <MemoryRouter initialEntries={[path]}>
      <ToastProvider>
        <AppProvider>
          <Routes>
            <Route path={route} element={element} />
          </Routes>
        </AppProvider>
      </ToastProvider>
    </MemoryRouter>,
  );
}

/** 在文章上選取一段文字（模擬滑鼠拖曳），`from`、`to` 是 UTF-16 位置。 */
export function selectInPassage(passage: HTMLElement, from: number, to: number) {
  const walker = document.createTreeWalker(passage, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let offset = 0;
  let startSet = false;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const len = node.textContent?.length ?? 0;
    if (!startSet && from <= offset + len) {
      range.setStart(node, from - offset);
      startSet = true;
    }
    if (to <= offset + len) {
      range.setEnd(node, to - offset);
      break;
    }
    offset += len;
  }
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
}
