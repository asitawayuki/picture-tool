import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { osColorScheme, setWindowTheme } from "../api";
import {
  OS_SCHEME_CHANGED_EVENT,
  OS_SCHEME_STORAGE_KEY,
  THEME_STORAGE_KEY,
  nextTheme,
  parseOsScheme,
  parseTheme,
  resolveTheme,
  type ColorScheme,
  type ThemePreference,
} from "./theme";
import { readStorage, writeStorage } from "./storage";

/**
 * テーマ設定の保持・永続化と、OS 追従の購読。
 *
 * **`data-theme` は常に明示的に書く。** メディアクエリ任せ（属性を消す）にしないのは、
 * Linux では WebKitGTK の `prefers-color-scheme` が GNOME の設定を映さないため
 * （実測の詳細は `gui/src/theme.rs` 冒頭）。OS の値は Rust から貰う。
 *
 * localStorage が使えない環境でも動く（`storage.ts`。読めなければ既定、書けなければ
 * 黙って捨てる）。
 */
/** Tauri の外（`vite dev` / e2e）ではメディアクエリしか手がかりが無い */
const DARK_QUERY = "(prefers-color-scheme: dark)";

export function createTheme() {
  let preference = $state<ThemePreference>(parseTheme(readStorage(THEME_STORAGE_KEY)));

  // 起動直後は Rust の応答が来るまで OS の値が分からない。前回の値を先に当てて、
  // ダークのデスクトップで白い画面が一瞬出るのを防ぐ（外れても次に訂正される）
  let osScheme = $state<ColorScheme>(parseOsScheme(readStorage(OS_SCHEME_STORAGE_KEY)));

  let resolved = $derived(resolveTheme(preference, osScheme));

  /**
   * いまの状態を画面とウィンドウへ反映する。
   *
   * **引数を取らない。** 状態を変える経路ごとに「何を渡すか」を書くと、
   * 渡し間違いが型検査を素通りして「`resolved` は正しいのに画面だけ古い」になる。
   *
   * `$effect` にしていないのは、effect が DOM 更新の後に走るため。起動時に
   * 一度ライトで描いてから暗くなる ── 前回値のキャッシュで避けたはずの
   * ちらつきが戻ってしまう。
   *
   * ウィンドウ側にも渡すのは、ネイティブのダイアログ（削除確認・書き込み承認）と
   * 窓枠をアプリ内の明暗に揃えるため（反映されるのは Linux だけ。理由は
   * `gui/src/theme.rs` の `set_window_theme`）。Tauri の外では失敗するので黙って捨てる。
   */
  function commit(): void {
    const scheme = resolveTheme(preference, osScheme);
    document.documentElement.dataset.theme = scheme;
    void setWindowTheme(scheme).catch(() => {});
  }

  commit();

  function setOsScheme(next: ColorScheme): void {
    // 起動直後はキャッシュと問い合わせの結果が一致するのが普通で、
    // 素通しにするとウィンドウへ同じテーマを 2 度渡すことになる
    if (next === osScheme) return;
    osScheme = next;
    writeStorage(OS_SCHEME_STORAGE_KEY, next);
    commit();
  }

  return {
    get preference(): ThemePreference {
      return preference;
    },

    /** 実際に適用されている明暗。`system` を解決した後の値 */
    get resolved(): ColorScheme {
      return resolved;
    },

    /** rail のボタン。押すたび システム → ライト → ダーク と巡る */
    cycle(): void {
      preference = nextTheme(preference);
      writeStorage(THEME_STORAGE_KEY, preference);
      commit();
    },

    /**
     * OS の現在値を取り込み、変化の購読を始める。
     *
     * Tauri の外（`vite dev` / e2e）ではコマンドが reject するので、
     * メディアクエリへ落ちる。戻り値は購読の解除
     * （`onMount` に返せるよう、`convertRun` の購読と同じ同期の形にしてある）。
     */
    start(): () => void {
      let unlisten: UnlistenFn | null = null;
      let detachMedia: (() => void) | null = null;
      let disposed = false;

      const fallbackToMedia = () => {
        if (disposed) return;
        // ここへ来るのは Tauri の外＝ブラウザの中なので matchMedia は必ずある
        const query = matchMedia(DARK_QUERY);
        const onChange = () => setOsScheme(query.matches ? "dark" : "light");
        onChange();
        query.addEventListener("change", onChange);
        detachMedia = () => query.removeEventListener("change", onChange);
      };

      osColorScheme()
        .then((scheme) => {
          if (disposed) return;
          setOsScheme(parseOsScheme(scheme));
          return listen<string>(OS_SCHEME_CHANGED_EVENT, (event) => {
            setOsScheme(parseOsScheme(event.payload));
          }).then((fn) => {
            if (disposed) fn();
            else unlisten = fn;
          });
        })
        // 失敗しても手動の切替は生きているのでトーストは出さない
        .catch(fallbackToMedia);

      return () => {
        disposed = true;
        unlisten?.();
        detachMedia?.();
      };
    },
  };
}
