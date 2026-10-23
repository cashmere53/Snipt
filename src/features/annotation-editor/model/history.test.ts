import { describe, expect, it } from "vitest";
import type { AnnotationDocument } from "./document";
import { canRedo, canUndo, commit, createHistory, HISTORY_LIMIT, redo, undo } from "./history";
import { addShape } from "./operations";
import { arrow, blank } from "./testing";

/** A history of `n` commits, each adding one arrow `a1`, `a2`, ... */
function history(n: number) {
  let h = createHistory(blank());
  for (let i = 1; i <= n; i++) {
    h = commit(h, addShape(h.present, arrow(`a${i}`)));
  }
  return h;
}

const count = (doc: AnnotationDocument) => doc.shapes.length;

describe("history", () => {
  it("undoes more than 100 of 150 commits and redoes them", () => {
    let h = history(150);
    for (let i = 0; i < 120; i++) {
      expect(canUndo(h)).toBe(true);
      h = undo(h);
    }
    expect(count(h.present)).toBe(30);
    for (let i = 0; i < 120; i++) {
      h = redo(h);
    }
    expect(count(h.present)).toBe(150);
    expect(canRedo(h)).toBe(false);
  });

  it("drops the redo steps on a new commit", () => {
    let h = undo(undo(history(3)));
    expect(count(h.present)).toBe(1);
    expect(canRedo(h)).toBe(true);
    h = commit(h, addShape(h.present, arrow("b")));
    expect(canRedo(h)).toBe(false);
    expect(redo(h)).toBe(h);
    expect(h.present.shapes.map((s) => s.id)).toEqual(["a1", "b"]);
  });

  it("forgets the oldest steps beyond the limit", () => {
    let h = history(HISTORY_LIMIT + 5);
    expect(h.past).toHaveLength(HISTORY_LIMIT);
    while (canUndo(h)) {
      h = undo(h);
    }
    expect(count(h.present)).toBe(5);
  });

  it("does not record the same document", () => {
    const h = history(2);
    expect(commit(h, h.present)).toBe(h);
  });

  it("does nothing at either end", () => {
    const h = createHistory(blank());
    expect(undo(h)).toBe(h);
    expect(redo(h)).toBe(h);
  });
});
