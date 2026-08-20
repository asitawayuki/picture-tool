/**
 * 部品確認用エントリ（dev 専用）。`vite dev` から /gallery.html で開く。
 *
 * vite.config.ts の build.rollupOptions.input には追加しないこと。
 * 追加すると dist/gallery.html とその chunk が生成され、frontendDist ごと
 * アプリのリソースに同梱されてしまう（spec §6）。
 */
import "./styles/tokens.css";
import "./app.css";
import Gallery from "./Gallery.svelte";
import { mount } from "svelte";

export default mount(Gallery, { target: document.getElementById("gallery")! });
