/**
 * 承認済みフォルダーの判定。**表示を決めるためだけのもの。**
 *
 * 本当の境界は Rust 側（`gui/src/security.rs` の `writable_image`）にあり、
 * ここが誤って true を返しても保存は弾かれる。逆にここが厳しすぎると
 * 「承認したのに保存ボタンが押せない」になるので、緩さより一致を優先する。
 *
 * Windows の大文字小文字非区別までは見ていない。Rust 側が `canonicalize` した
 * 実体パスで判定するので、食い違った場合は保存時のエラーで気づける。
 */
function normalize(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function isUnder(root: string, path: string): boolean {
  const r = normalize(root);
  const p = normalize(path);
  return p === r || p.startsWith(`${r}/`);
}

export function anyAllows(roots: readonly string[], path: string): boolean {
  return roots.some((root) => isUnder(root, path));
}

/** ファイルパスから親フォルダーを取る。区切り文字は元の綴りのまま返す */
export function parentFolder(path: string): string {
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return cut <= 0 ? path : path.slice(0, cut);
}
