import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

// vite-plugin-svelte / svelte-check の共通設定。
// svelte-check はこのファイルが無いと vite.config.ts から設定を解決できずエラーになる。
export default {
  preprocess: vitePreprocess(),
};
