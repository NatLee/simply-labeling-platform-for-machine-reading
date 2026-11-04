import { vi } from "vitest";

export interface Call {
  method: string;
  path: string;
  body: unknown;
}

type Handler = (body: unknown, url: URL) => unknown;

/**
 * 換掉 `fetch`：依「方法 路徑」回傳假資料，並記下每個請求，讓測試檢查前端送了什麼。
 * 路徑不含查詢字串；找不到對應的 handler 時回 404，測試會看到錯誤訊息。
 */
export function fakeApi(routes: Record<string, Handler>) {
  const calls: Call[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = new URL(String(input), "http://localhost");
      const method = (init?.method ?? "GET").toUpperCase();
      const path = url.pathname.replace(/^\/api/, "");
      const body = typeof init?.body === "string" ? JSON.parse(init.body) : init?.body;
      calls.push({ method, path, body });
      const handler = routes[`${method} ${path}`];
      if (!handler) {
        return new Response(JSON.stringify({ error: `no fake for ${method} ${path}` }), { status: 404 });
      }
      const result = handler(body, url);
      if (result === undefined) return new Response(null, { status: 204 });
      return new Response(JSON.stringify(result), { status: 200, headers: { "content-type": "application/json" } });
    }),
  );
  return calls;
}
