import { useCallback, useEffect, useRef, useState } from "react";

/** 載入資料：回傳 data、錯誤、是否載入中，以及重新載入的函式。 */
export function useAsync<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const seq = useRef(0);
  const load = useCallback(fn, deps);

  const reload = useCallback(async () => {
    const id = ++seq.current;
    setLoading(true);
    try {
      const v = await load();
      if (id === seq.current) {
        setData(v);
        setError(null);
      }
    } catch (e) {
      if (id === seq.current) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (id === seq.current) setLoading(false);
    }
  }, [load]);

  useEffect(() => {
    void reload();
  }, [reload]);

  return { data, setData, error, loading, reload };
}

/** 焦點在輸入框裡時，單鍵快捷鍵不要觸發（不然打字會一直跳）。 */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target.isContentEditable;
}

export interface Hotkey {
  /** `KeyboardEvent.key`，單一字母用小寫，例如 "n"、"ArrowRight"、"Enter"、"?"。 */
  key: string;
  /** Ctrl（Mac 是 ⌘）。 */
  mod?: boolean;
  shift?: boolean;
  /** 在輸入框裡也要觸發（例如 Esc、Ctrl+Enter）。 */
  whileTyping?: boolean;
  /** 條件不成立時不攔截這個按鍵（例如沒有選取文字時，Enter 照常按下按鈕）。 */
  when?: () => boolean;
  run: (e: KeyboardEvent) => void;
}

/** 註冊一組快捷鍵。每次 render 都拿最新的 handler，不用擔心閉包拿到舊的 state。 */
export function useHotkeys(keys: Hotkey[], enabled = true) {
  const ref = useRef(keys);
  ref.current = keys;
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.isComposing || e.altKey) return;
      const mod = e.ctrlKey || e.metaKey;
      const typing = isTyping(e.target);
      for (const k of ref.current) {
        if (k.key.toLowerCase() !== e.key.toLowerCase()) continue;
        if (Boolean(k.mod) !== mod) continue;
        if (k.shift !== undefined && k.shift !== e.shiftKey) continue;
        if (typing && !k.whileTyping) continue;
        if (k.when && !k.when()) continue;
        e.preventDefault();
        k.run(e);
        return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled]);
}

/** 存在瀏覽器裡的設定（例如目前的標註者）。 */
export function useStoredState<T>(key: string, initial: T): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw === null ? initial : (JSON.parse(raw) as T);
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (v: T) => {
      setValue(v);
      try {
        localStorage.setItem(key, JSON.stringify(v));
      } catch {
        // 無痕模式等情況存不了，只在這次使用
      }
    },
    [key],
  );
  return [value, set];
}
