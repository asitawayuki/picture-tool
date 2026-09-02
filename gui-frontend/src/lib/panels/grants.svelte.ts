import { grantMetadataEditing } from "../api";
import { anyAllows } from "./grants";

/**
 * 書き込みを承認済みのフォルダー。
 *
 * **保持は表示のためだけ。** 実際の許可は Rust 側の `WritableRoots` にあり、
 * アプリを終了すると消える。だからここも永続化しない ── 永続化すると
 * 「承認済みと出ているのに保存できない」状態が起動直後に必ず生まれる。
 */
export function createGrantStore() {
  let roots = $state<string[]>([]);

  return {
    get roots() {
      return roots;
    },
    allows(path: string): boolean {
      return anyAllows(roots, path);
    },
    async request(folder: string): Promise<boolean> {
      // 保持するのは Rust が解決した実体パス。送った生の文字列を保持すると
      // 境界の判定とずれて「承認済み表示なのに保存が弾かれる」になる
      const granted = await grantMetadataEditing(folder);
      if (granted !== null && !roots.includes(granted)) roots = [...roots, granted];
      return granted !== null;
    },
  };
}
