// 答案範圍的計算。位置一律用「字元」（Unicode code point），和後端、SQuAD 一樣；
// 瀏覽器的選取範圍是 UTF-16 位置，在這裡換算。

/** 字元位置的範圍，`end` 不含。 */
export interface CharSpan {
  start: number;
  end: number;
  text: string;
}

export function charLength(s: string): number {
  return Array.from(s).length;
}

/** UTF-16 位置換成字元位置。emoji 這類字元在 UTF-16 佔兩個單位，在字元位置只佔一個。 */
export function utf16ToChar(text: string, utf16: number): number {
  return Array.from(text.slice(0, utf16)).length;
}

export function sliceChars(text: string, start: number, end: number): string {
  return Array.from(text).slice(start, end).join("");
}

/** 去掉範圍前後的空白（雙擊選字或拖曳時常多選到空白）。 */
export function trimSpan(text: string, start: number, end: number): CharSpan {
  const chars = Array.from(text);
  let s = Math.max(0, Math.min(start, chars.length));
  let e = Math.max(s, Math.min(end, chars.length));
  while (s < e && /\s/u.test(chars[s])) s++;
  while (e > s && /\s/u.test(chars[e - 1])) e--;
  return { start: s, end: e, text: chars.slice(s, e).join("") };
}

/**
 * 把畫面上的選取範圍換成文章裡的字元位置。`container` 裡只能有文章的文字
 * （highlight 用的 `<mark>` 沒關係，但不能有額外的文字節點）。
 * 選取超出文章的部分會被截掉；沒有選到文章時回 `null`。
 */
export function selectionToSpan(container: HTMLElement, text: string, sel: Selection | null): CharSpan | null {
  if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return null;
  const range = sel.getRangeAt(0);
  const whole = document.createRange();
  whole.selectNodeContents(container);
  // 完全不在文章裡
  if (
    range.compareBoundaryPoints(Range.END_TO_START, whole) >= 0 ||
    range.compareBoundaryPoints(Range.START_TO_END, whole) <= 0
  ) {
    return null;
  }
  const startsInside = range.compareBoundaryPoints(Range.START_TO_START, whole) >= 0;
  const endsInside = range.compareBoundaryPoints(Range.END_TO_END, whole) <= 0;

  const before = document.createRange();
  before.selectNodeContents(container);
  let startU16 = 0;
  if (startsInside) {
    before.setEnd(range.startContainer, range.startOffset);
    startU16 = before.toString().length;
  }
  let endU16 = text.length;
  if (endsInside) {
    before.setEnd(range.endContainer, range.endOffset);
    endU16 = before.toString().length;
  }
  const span = trimSpan(text, utf16ToChar(text, startU16), utf16ToChar(text, endU16));
  return span.end > span.start ? span : null;
}

/** 文章上要標顏色的一段。 */
export interface Mark {
  start: number;
  end: number;
  /** 顏色的編號（第幾個問題），見 `lib/tones.ts`。 */
  tone: number;
  /** 目前正在編輯的問題，畫得比較明顯。 */
  active?: boolean;
  /** AI 建議（虛線框）。 */
  suggestion?: boolean;
  /** 還沒加進答案的選取範圍。 */
  pending?: boolean;
  /** 點了標記要選到哪個問題或建議（由呼叫端決定格式）。 */
  ref?: string;
}

export interface Segment {
  start: number;
  end: number;
  text: string;
  marks: Mark[];
}

/** 依所有標記的邊界把文章切段，每段記下蓋在它上面的標記。重疊的標記也能正確顯示。 */
export function segments(text: string, marks: Mark[]): Segment[] {
  const chars = Array.from(text);
  const valid = marks.filter((m) => m.end > m.start && m.start >= 0 && m.end <= chars.length);
  const cuts = new Set<number>([0, chars.length]);
  for (const m of valid) {
    cuts.add(m.start);
    cuts.add(m.end);
  }
  const points = [...cuts].sort((a, b) => a - b);
  const out: Segment[] = [];
  for (let i = 0; i < points.length - 1; i++) {
    const start = points[i];
    const end = points[i + 1];
    out.push({
      start,
      end,
      text: chars.slice(start, end).join(""),
      marks: valid.filter((m) => m.start <= start && m.end >= end),
    });
  }
  return out;
}

/** 文章裡 `start` 附近的一小段，審核頁用來顯示答案的上下文。 */
export function snippet(text: string, start: number, end: number, radius = 80): { start: number; end: number } {
  const len = charLength(text);
  return { start: Math.max(0, start - radius), end: Math.min(len, end + radius) };
}
