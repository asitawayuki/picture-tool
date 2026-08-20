import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { cancelProcessing, pickOutputFolder, processImages } from "../api";
import { describeError, toast } from "../toasts.svelte";
import type {
  ExifFrameConfig,
  ImageEntry,
  ProcessBatchResponse,
  ProcessingConfig,
  ProgressPayload,
} from "../types";

export interface ConvertResult {
  /** 変換を依頼した画像。キャンセル分は results にも failures にも現れない */
  requested: ImageEntry[];
  response: ProcessBatchResponse;
  /** 利用者がキャンセルした場合、未処理分は「失敗」ではないので区別する */
  cancelled: boolean;
}

/**
 * 変換モードの設定と実行。
 *
 * 設定（出力先・変換設定・Exif フレームの有無）まで持たせてあるのは、
 * それらを使うのが「変換の実行」だけであり、App.svelte に置くと
 * spec §3-5 の「4 状態とパネルの差し替えだけ」が崩れるため。
 * 選択（どの写真を変換するか）は App が持ち、run の引数で渡す。
 */
export function createConvertRun() {
  let processing = $state(false);
  let progress = $state<ProgressPayload | null>(null);
  let result = $state<ConvertResult | null>(null);
  let cancelRequested = false;

  let outputFolder = $state("");
  let config = $state<ProcessingConfig>({
    mode: "crop",
    bg_color: "white",
    quality: 90,
    max_size_mb: 8,
    delete_originals: false,
    max_width: null,
  });
  let exifFrameEnabled = $state(false);

  /** 確認待ちの依頼。元ファイル削除が有効なときだけ埋まる */
  let pending = $state<{
    requested: ImageEntry[];
    exifFrameConfig: ExifFrameConfig | null;
  } | null>(null);

  async function start(
    requested: ImageEntry[],
    exifFrameConfig: ExifFrameConfig | null
  ): Promise<void> {
    if (processing || requested.length === 0 || outputFolder === "") return;

    processing = true;
    cancelRequested = false;
    progress = { current: 0, total: requested.length, file_name: "" };

    try {
      const files = requested.map((img) => img.path);
      const response = await processImages(files, outputFolder, config, exifFrameConfig);
      result = { requested, response, cancelled: cancelRequested };
    } catch (e) {
      toast.error(`変換に失敗しました: ${describeError(e)}`);
    } finally {
      processing = false;
      progress = null;
    }
  }

  return {
    get processing() {
      return processing;
    },
    get config() {
      return config;
    },
    get outputFolder() {
      return outputFolder;
    },
    get exifFrameEnabled() {
      return exifFrameEnabled;
    },
    set exifFrameEnabled(next: boolean) {
      exifFrameEnabled = next;
    },
    /** 元ファイル削除の確認待ちなら、その対象枚数。待っていなければ null */
    get confirming(): number | null {
      return pending === null ? null : pending.requested.length;
    },
    get progress() {
      return progress;
    },
    get result() {
      return result;
    },

    /** onMount の返り値にそのまま渡せるクリーンアップを返す */
    subscribeProgress(): () => void {
      let unlisten: UnlistenFn | null = null;
      let disposed = false;

      listen<ProgressPayload>("processing-progress", (event) => {
        progress = event.payload;
      })
        .then((fn) => {
          if (disposed) fn();
          else unlisten = fn;
        })
        .catch((e) => {
          toast.error(`進捗の購読に失敗しました: ${describeError(e)}`);
        });

      return () => {
        disposed = true;
        unlisten?.();
      };
    },

    /** 出力先を選ぶ。ダイアログは Rust 側が開く（S6-H8） */
    async pickOutput(startFrom?: string): Promise<void> {
      try {
        // ここで選ばれたフォルダーだけがバックエンドの書き込み許可対象になる
        const selected = await pickOutputFolder(startFrom || undefined);
        if (selected) outputFolder = selected;
      } catch (e) {
        toast.error(`出力先の選択に失敗しました: ${describeError(e)}`);
      }
    },

    /**
     * 変換を依頼する。Exif フレームは pad モードで有効にしたときだけ適用する。
     * 元ファイルの一括削除は取り消せないため、その場合だけ確認を挟む。
     */
    request(requested: ImageEntry[], preset: ExifFrameConfig | null): void {
      const exifFrameConfig =
        config.mode === "pad" && exifFrameEnabled ? preset : null;
      if (config.delete_originals) {
        pending = { requested, exifFrameConfig };
        return;
      }
      void start(requested, exifFrameConfig);
    },

    /** 確認ダイアログの「削除して変換」 */
    confirm(): void {
      const next = pending;
      pending = null;
      if (next) void start(next.requested, next.exifFrameConfig);
    },

    /** 確認ダイアログの「キャンセル」 */
    dismissConfirm(): void {
      pending = null;
    },

    async cancel(): Promise<void> {
      try {
        cancelRequested = true;
        await cancelProcessing();
      } catch (e) {
        toast.error(`キャンセルに失敗しました: ${describeError(e)}`);
      }
    },

    dismissResult() {
      result = null;
    },
  };
}
