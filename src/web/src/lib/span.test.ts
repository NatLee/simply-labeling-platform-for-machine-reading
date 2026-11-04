import { describe, expect, it } from "vitest";
import { segments, selectionToSpan, sliceChars, trimSpan, utf16ToChar } from "./span";

// 「😀」在 UTF-16 佔 2 個單位、字元位置只佔 1 個；和後端測試用同一段文字。
const TEXT = "台北😀101 高 508 公尺，在 2004 年完工。";

describe("utf16ToChar", () => {
  it("counts an emoji as one character", () => {
    expect(utf16ToChar(TEXT, 10)).toBe(9);
    expect(sliceChars(TEXT, 9, 15)).toBe("508 公尺");
  });
});

describe("trimSpan", () => {
  it("drops surrounding whitespace", () => {
    // 位置 8 是「高」後面的空白
    expect(trimSpan(TEXT, 8, 15)).toEqual({ start: 9, end: 15, text: "508 公尺" });
    // 和後端 text.rs 的測試同一組數字
    expect(trimSpan("say  hello  world", 3, 12)).toEqual({ start: 5, end: 10, text: "hello" });
    expect(trimSpan("   ", 0, 3)).toEqual({ start: 3, end: 3, text: "" });
  });
});

describe("segments", () => {
  it("splits overlapping marks at every boundary", () => {
    const segs = segments("abcdef", [
      { start: 1, end: 4, tone: 0 },
      { start: 2, end: 5, tone: 1 },
    ]);
    expect(segs.map((s) => [s.text, s.marks.map((m) => m.tone)])).toEqual([
      ["a", []],
      ["b", [0]],
      ["cd", [0, 1]],
      ["e", [1]],
      ["f", []],
    ]);
  });

  it("ignores marks outside the text", () => {
    expect(segments("ab", [{ start: 1, end: 9, tone: 0 }])).toHaveLength(1);
  });
});

describe("selectionToSpan", () => {
  it("converts a DOM selection across marks into character offsets", () => {
    const div = document.createElement("div");
    // 和 PassageView 一樣：文字分在好幾個節點，中間有 <mark>
    div.innerHTML = "台北😀101 <mark>高 508</mark> 公尺，在 2004 年完工。";
    document.body.appendChild(div);
    const mark = div.querySelector("mark")!;
    const range = document.createRange();
    // 從 <mark> 裡「508」的開頭（「高 」之後）選到後面文字節點的「公尺」結尾
    range.setStart(mark.firstChild!, 2);
    range.setEnd(mark.nextSibling!, 3);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);
    expect(selectionToSpan(div, TEXT, sel)).toEqual({ start: 9, end: 15, text: "508 公尺" });
    div.remove();
  });

  it("returns null when nothing in the passage is selected", () => {
    const div = document.createElement("div");
    div.textContent = TEXT;
    const other = document.createElement("p");
    other.textContent = "outside";
    document.body.append(div, other);
    const range = document.createRange();
    range.selectNodeContents(other);
    const sel = window.getSelection()!;
    sel.removeAllRanges();
    sel.addRange(range);
    expect(selectionToSpan(div, TEXT, sel)).toBeNull();
    div.remove();
    other.remove();
  });
});
