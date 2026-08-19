import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { cancelProcessing, processImages } from "../api";
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

export function createConvertRun() {
  let processing = $state(false);
  let progress = $state<ProgressPayload | null>(null);
  let result = $state<ConvertResult | null>(null);
  let cancelRequested = false;

  return {
    get processing() {
      return processing;
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

    async run(
      requested: ImageEntry[],
      outputFolder: string,
      config: ProcessingConfig,
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
