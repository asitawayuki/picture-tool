import { defineConfig, devices } from "@playwright/test";

/**
 * 検証は vite dev サーバーに当てる（spec §7-3）。
 *
 * Tauri の外なので `window.__TAURI_INTERNALS__` が無く、`invoke` は全部 reject する。
 * エラー経路の検証にはこれがそのまま使える。見た目の検証にはスタブを注入する
 * （e2e/stub.ts。Task 9 で追加）。
 *
 * CI では走らせない。CI にブラウザバイナリを入れていないため。
 *
 * **ポートは e2e 専用（既定 5174）にして、必ず自分でサーバーを立てる。**
 * `vite.config.ts` は `port: 5173` / `strictPort: true` で、そこは
 * `make dev`（Tauri の dev サーバー）が使う。5173 を `reuseExistingServer: true`
 * で共有すると、`make dev` を上げたまま e2e を走らせたときに
 * **別のチェックアウト（worktree）の画面を検証してしまう**。
 * 落ちない代わりに嘘の緑が出る種類の事故なので、port ごと分ける。
 *
 * 5174 が別のプロジェクトに使われている環境では `E2E_PORT` で退避させる
 * （`E2E_PORT=5199 bun run e2e`）。**`reuseExistingServer` は false のまま**なので、
 * どのポートを選んでも「自分で立てたサーバーを見る」性質は変わらない。
 */
const PORT = Number(process.env.E2E_PORT ?? 5174);
const ORIGIN = `http://localhost:${PORT}`;

export default defineConfig({
  testDir: "./e2e",
  // 同じ vite dev サーバーを共有するので直列に走らせる
  fullyParallel: false,
  workers: 1,
  timeout: 30_000,
  use: {
    baseURL: ORIGIN,
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // 既定ウィンドウ寸法（tauri.conf.json / spec §3-1）に合わせる。
        // **device の後に置くこと** ── `devices["Desktop Chrome"]` は
        // viewport 1280x720 を持っており、project の use は最上位の use を
        // 上書きするので、最上位に書くと黙って 1280 で走る（spec §3-1 の
        // 列数の実測表は 1440 のもの。Task 15 の計測も同じ幅で行う）
        viewport: { width: 1440, height: 800 },
      },
    },
  ],
  webServer: {
    // vite の CLI 引数は vite.config.ts の server.port を上書きする
    command: `bunx vite dev --port ${PORT} --strictPort`,
    url: ORIGIN,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
