// 每個問題一個顏色，讓文章上不同問題的答案分得出來。
// Tailwind 只會產生原始碼裡完整寫出來的 class，所以這裡每個 class 都要寫完整。

export interface Tone {
  /** 一般的答案底色 */
  mark: string;
  /** 正在編輯的問題 */
  strong: string;
  /** 問題卡片左邊的色點 */
  dot: string;
}

export const TONES: Tone[] = [
  {
    mark: "bg-violet-200/70 dark:bg-violet-500/25",
    strong: "bg-violet-300/90 ring-2 ring-violet-500 dark:bg-violet-500/45 dark:ring-violet-400",
    dot: "bg-violet-500",
  },
  {
    mark: "bg-amber-200/70 dark:bg-amber-500/25",
    strong: "bg-amber-300/90 ring-2 ring-amber-500 dark:bg-amber-500/45 dark:ring-amber-400",
    dot: "bg-amber-500",
  },
  {
    mark: "bg-emerald-200/70 dark:bg-emerald-500/25",
    strong: "bg-emerald-300/90 ring-2 ring-emerald-500 dark:bg-emerald-500/45 dark:ring-emerald-400",
    dot: "bg-emerald-500",
  },
  {
    mark: "bg-sky-200/70 dark:bg-sky-500/25",
    strong: "bg-sky-300/90 ring-2 ring-sky-500 dark:bg-sky-500/45 dark:ring-sky-400",
    dot: "bg-sky-500",
  },
  {
    mark: "bg-rose-200/70 dark:bg-rose-500/25",
    strong: "bg-rose-300/90 ring-2 ring-rose-500 dark:bg-rose-500/45 dark:ring-rose-400",
    dot: "bg-rose-500",
  },
  {
    mark: "bg-lime-200/70 dark:bg-lime-500/25",
    strong: "bg-lime-300/90 ring-2 ring-lime-500 dark:bg-lime-500/45 dark:ring-lime-400",
    dot: "bg-lime-500",
  },
  {
    mark: "bg-fuchsia-200/70 dark:bg-fuchsia-500/25",
    strong: "bg-fuchsia-300/90 ring-2 ring-fuchsia-500 dark:bg-fuchsia-500/45 dark:ring-fuchsia-400",
    dot: "bg-fuchsia-500",
  },
  {
    mark: "bg-cyan-200/70 dark:bg-cyan-500/25",
    strong: "bg-cyan-300/90 ring-2 ring-cyan-500 dark:bg-cyan-500/45 dark:ring-cyan-400",
    dot: "bg-cyan-500",
  },
];

export function tone(i: number): Tone {
  return TONES[((i % TONES.length) + TONES.length) % TONES.length];
}

/** AI 建議用虛線框，和人標的答案分開。 */
export const SUGGESTION_MARK =
  "bg-brand-100/60 outline-2 outline-dashed outline-brand-400 dark:bg-brand-500/15 dark:outline-brand-400";
