import { describe, expect, test } from "bun:test";
import { anyAllows, isUnder, parentFolder } from "./grants";

describe("承認済みフォルダーの判定（表示用）", () => {
  test("配下のファイルは許可される", () => {
    expect(isUnder("/home/u/photos", "/home/u/photos/a.jpg")).toBe(true);
    expect(isUnder("/home/u/photos", "/home/u/photos/2026/a.jpg")).toBe(true);
  });

  // 名前が前方一致するだけの兄弟は別のフォルダー。
  // ここを落とすと「承認済み」と表示して、保存でいきなり Rust に弾かれる
  test("名前が前方一致する兄弟は許可されない", () => {
    expect(isUnder("/home/u/photos", "/home/u/photos-old/a.jpg")).toBe(false);
  });

  test("末尾のスラッシュの有無で結果が変わらない", () => {
    expect(isUnder("/home/u/photos/", "/home/u/photos/a.jpg")).toBe(true);
  });

  test("Windows の区切り文字でも判定できる", () => {
    expect(isUnder("C:\\Users\\u\\photos", "C:\\Users\\u\\photos\\a.jpg")).toBe(true);
  });

  test("anyAllows はどれか 1 つに当たれば true", () => {
    expect(anyAllows(["/a", "/b"], "/b/x.jpg")).toBe(true);
    expect(anyAllows(["/a", "/b"], "/c/x.jpg")).toBe(false);
    expect(anyAllows([], "/a/x.jpg")).toBe(false);
  });

  test("parentFolder は両方の区切り文字を扱う", () => {
    expect(parentFolder("/home/u/photos/a.jpg")).toBe("/home/u/photos");
    expect(parentFolder("C:\\Users\\u\\photos\\a.jpg")).toBe("C:\\Users\\u\\photos");
  });
});
