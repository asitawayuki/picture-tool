import { describe, expect, test } from "bun:test";
import { emptyValues, isEditableImage, sameValues, toMetadata, toValues } from "./metadataValues";

describe("フォーム値 ⇄ EditableMetadata", () => {
  test("null は空欄・未評価として見せる", () => {
    expect(toValues({ title: null, comment: null, rating: null })).toEqual(emptyValues());
  });

  // null（タグが無い）と ""（空文字のタグがある）は Rust 側で別物。
  // 空欄で保存したら「タグを消す」＝ null を送る
  test("空欄は null にして送る", () => {
    expect(toMetadata({ title: "", comment: "", rating: 0 })).toEqual({
      title: null,
      comment: null,
      rating: null,
    });
  });

  test("入力された値はそのまま送る", () => {
    expect(toMetadata({ title: "港", comment: "順光", rating: 4 })).toEqual({
      title: "港",
      comment: "順光",
      rating: 4,
    });
  });

  // ★0 は「未評価」であって 0 という評価ではない（RatingPercent に表現できない）
  test("★0 は null になる", () => {
    expect(toMetadata({ title: "港", comment: "", rating: 0 }).rating).toBeNull();
  });

  test("往復しても値が変わらない", () => {
    const metadata = { title: "港", comment: "順光", rating: 3 };
    expect(toMetadata(toValues(metadata))).toEqual(metadata);
  });

  test("sameValues は 3 項目すべてを見る", () => {
    const base = { title: "a", comment: "b", rating: 1 };
    expect(sameValues(base, { ...base })).toBe(true);
    expect(sameValues(base, { ...base, title: "z" })).toBe(false);
    expect(sameValues(base, { ...base, comment: "z" })).toBe(false);
    expect(sameValues(base, { ...base, rating: 5 })).toBe(false);
  });
});

describe("編集できるファイルの判定", () => {
  // Rust 側（security.rs の readable_jpeg）と同じ条件。
  // ここが緩いと「編集できる顔をして保存で弾かれる」になる
  test("JPEG だけを編集できる", () => {
    for (const name of ["a.jpg", "a.JPG", "a.jpeg", "a.JPEG"]) {
      expect(isEditableImage(name)).toBe(true);
    }
    for (const name of ["a.png", "a.webp", "a.jpg.png", "a"]) {
      expect(isEditableImage(name)).toBe(false);
    }
  });
});
