import type { Page } from "@playwright/test";

export interface StubOptions {
  /** list_images が返す枚数 */
  imageCount?: number;
  /**
   * `render_exif_frame_preview` が返す警告。
   *
   * ここに載るのは**アセット由来**の警告（カスタム `model_map` の不備など）で、
   * spec §5-3 では「返ってくるので従来どおり toast する」側である。
   * フレーム描画由来の警告は Rust 側で捨てられるため webview には届かない。
   */
  frameWarnings?: string[];
}

/**
 * Tauri の IPC をスタブする。
 *
 * @tauri-apps/api の invoke は window.__TAURI_INTERNALS__.invoke へ委譲し、
 * listen は transformCallback + `plugin:event|listen` を経由する。
 * この 2 つを用意すれば webview の中の挙動をそのまま再現できる。
 */
export async function installTauriStub(page: Page, options: StubOptions = {}) {
  await page.addInitScript((opts: { imageCount: number; frameWarnings: string[] }) => {
    const { imageCount, frameWarnings } = opts;
    /** サムネイルは 24 種類だけ作って使い回す。
     *  全部同じにするとデコードが 1 回で済んでしまい、スクロール計測の
     *  負荷が実際より軽く出る。全部別にすると生成自体が計測を汚す。 */
    const POOL_SIZE = 24;
    const pool = new Map<string, string>();

    function jpegFor(index: number, size: number): string {
      const key = `${index % POOL_SIZE}:${size}`;
      const cached = pool.get(key);
      if (cached) return cached;

      const canvas = document.createElement("canvas");
      canvas.width = size;
      canvas.height = Math.round((size * 5) / 4);
      const ctx = canvas.getContext("2d")!;
      const hue = ((index % POOL_SIZE) * 360) / POOL_SIZE;
      const gradient = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
      gradient.addColorStop(0, `hsl(${hue} 70% 60%)`);
      gradient.addColorStop(1, `hsl(${(hue + 60) % 360} 70% 25%)`);
      ctx.fillStyle = gradient;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#fff";
      ctx.font = `${Math.round(size / 6)}px sans-serif`;
      ctx.fillText(String(index % POOL_SIZE), size / 8, size / 3);

      const base64 = canvas.toDataURL("image/jpeg", 0.8).split(",")[1];
      pool.set(key, base64);
      return base64;
    }

    function indexOfPath(path: string): number {
      const m = /(\d+)\.jpg$/.exec(path);
      return m ? Number(m[1]) : 0;
    }

    /** フォルダーごとに別のパスを返す。同じパスを返すと、フォルダーを変えても
     *  キャッシュが当たってしまい「取り直している」ことを検査できない */
    function imagesFor(folder: string) {
      const dir = folder.replace(/\/+$/, "") || "/photos";
      return Array.from({ length: imageCount }, (_, i) => ({
        name: `photo-${String(i).padStart(4, "0")}.jpg`,
        path: `${dir}/${i}.jpg`,
        width: 4000,
        height: 3000,
        size_bytes: 4_500_000,
      }));
    }

    const presets = [
      {
        name: "default",
        position: "auto",
        items: {
          maker_logo: true, lens_brand_logo: true, camera_model: true,
          lens_model: true, focal_length: true, f_number: true,
          shutter_speed: true, iso: true, date_taken: false, custom_text: false,
        },
        font: { font_path: null, primary_size: 0.025, secondary_size: 0.018 },
        custom_text: "",
      },
    ];

    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextCallbackId = 1;

    const handlers: Record<string, (args: any) => unknown> = {
      list_drives: () => ["/"],
      list_directory: () => [
        { name: "photos", path: "/photos", is_dir: true, is_image: false },
        { name: "archive", path: "/archive", is_dir: true, is_image: false },
      ],
      list_images: (a) => imagesFor(a.path ?? "/photos"),
      // 取得の記録を残す。サムネイルは「出ている」だけでは検査にならず、
      // どの解像度で何回取りに来たかを見ないと再取得の有無が判らない
      get_thumbnail: (a) => {
        const log = ((window as any).__thumbnailRequests ??= []);
        log.push({ path: a.path, maxDimension: a.maxDimension });
        return jpegFor(indexOfPath(a.path), a.maxDimension);
      },
      get_full_image: (a) => jpegFor(indexOfPath(a.path), 800),
      get_exif_info: () => ({
        camera_make: "SONY", camera_model: "ILCE-7M4", lens_model: "FE 35mm F1.4 GM",
        focal_length: "35mm", f_number: "f/1.4", shutter_speed: "1/250s",
        iso: 400, date_taken: "2026-08-19 10:00:00", orientation: 1,
      }),
      pick_output_folder: () => "/output",
      load_favorites: () => ["/photos"],
      save_favorites: () => null,
      // 変換の通し検査（Task 10 Step 7）のために、依頼された分だけ成功を返し、
      // 渡された引数を残す。空配列を返すと結果ダイアログが常に
      // 「0 成功 / N 未処理」になり、何を送ったかも見えない
      process_images: (a) => {
        (window as any).__lastProcessArgs = a;
        return {
          results: (a.files as string[]).map((input_path) => ({
            input_path,
            output_path: `${a.outputFolder}/${input_path.split("/").pop()}`,
            final_size_mb: 3.2,
            final_quality: 88,
            size_limit_exceeded: false,
            warnings: [],
          })),
          failures: [],
          warnings: [],
        };
      },
      cancel_processing: () => null,
      // 生成の回数を残す。「同じ警告が二度出ない」の検査は、二度目の生成が
      // 実際に起きたことを前提条件として見ないと、生成されていないだけで成立する
      render_exif_frame_preview: (a) => {
        const log = ((window as any).__framePreviewRequests ??= []);
        log.push({ path: a.path, bgColor: a.bgColor, config: a.config });
        return {
          data_url: `data:image/jpeg;base64,${jpegFor(0, 400)}`,
          warnings: frameWarnings,
        };
      },
      // プリセットは読み書きの往復を検査したい（Task 16 の改名）ので、
      // 配列を実際に書き換える。引数名は api.ts に合わせること
      // （save_preset は { config }、delete_preset は { name }）
      list_presets: () => presets,
      save_preset: (a) => {
        const i = presets.findIndex((p) => p.name === a.config.name);
        if (i >= 0) presets[i] = a.config;
        else presets.push(a.config);
        return null;
      },
      delete_preset: (a) => {
        const i = presets.findIndex((p) => p.name === a.name);
        if (i >= 0) presets.splice(i, 1);
        return null;
      },
      list_available_fonts: () => [
        { display_name: "同梱フォント", path: null, is_bundled: true },
      ],
      "plugin:event|listen": () => nextCallbackId++,
      "plugin:event|unlisten": () => null,
    };

    (window as any).__TAURI_INTERNALS__ = {
      transformCallback(callback: (payload: unknown) => void) {
        const id = nextCallbackId++;
        callbacks.set(id, callback);
        return id;
      },
      async invoke(cmd: string, args: any) {
        const handler = handlers[cmd];
        if (!handler) throw new Error(`stub: 未対応のコマンド ${cmd}`);
        return handler(args ?? {});
      },
    };
  }, { imageCount: options.imageCount ?? 24, frameWarnings: options.frameWarnings ?? [] });
}

/**
 * `localStorage` を**そのテストの最初のナビゲーションでだけ**空にする。
 *
 * `page.addInitScript(() => localStorage.clear())` を直に書いてはいけない。
 * init script は `page.reload()` を含む**すべてのナビゲーションで再実行される**ので、
 * 「幅を変えてリロードしても保たれる」「畳んだ状態がリロード後も残る」といった
 * 永続化の検証が、リロードのたびに消されて必ず落ちる。
 *
 * `sessionStorage` はリロードを跨いで残り、Playwright は 1 テスト 1 コンテキストなので、
 * これで「テストごとに 1 回だけ」が成立する。
 */
export async function clearStorageOnce(page: Page) {
  await page.addInitScript(() => {
    const FLAG = "__pt_storage_cleared";
    if (!sessionStorage.getItem(FLAG)) {
      localStorage.clear();
      sessionStorage.setItem(FLAG, "1");
    }
  });
}

/**
 * `Switch` を切り替える。
 *
 * `getByRole("checkbox", …).click()` は通らない ── `Switch` は透明な `input` の上に
 * `.track`（`position: relative`）が重なる構造で、当たり判定は DOM 順で後ろの
 * `.track` が取るため、Playwright の actionability チェックが
 * `.track intercepts pointer events` で落ちる（Task 4 Step 2 の注記）。
 * 可視ラベルをクリックすれば `label` の既定動作で `input` が切り替わる。
 */
export function toggleSwitch(page: Page, label: string) {
  return page.getByText(label, { exact: true }).click();
}

/**
 * `SegmentedButton` の選択肢を選ぶ。
 *
 * `getByRole("radio", …).click()` は通らない ── `SegmentedButton` の `input` は
 * `position: absolute; opacity: 0; pointer-events: none` で隠してあり、当たり判定は
 * 上に載る `.text` が取る。Playwright の actionability チェックが
 * `.text intercepts pointer events` で落ちる（`Switch` と同じ理由）。
 * 可視ラベルをクリックすれば `label` の既定動作で `input` が選ばれる。
 */
export function selectSegment(page: Page, label: string) {
  return page.getByText(label, { exact: true }).click();
}
