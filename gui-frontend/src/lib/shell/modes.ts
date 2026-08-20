export type AppMode = "convert" | "metadata" | "frame";

/**
 * rail の destination（spec §3-3）。
 *
 * ラベルは日本語。「メタデータ」ではなく「情報」にしてあるのは、
 * rail 幅 80px に収めるため（spec §8 の未確定項目に対する結論）。
 * 幅が変わる変更をしたら Step 9 の実測をやり直すこと。
 */
export const MODES: { value: AppMode; label: string; icon: string }[] = [
  { value: "convert", label: "変換", icon: "⇄" },
  { value: "metadata", label: "情報", icon: "ℹ" },
  { value: "frame", label: "フレーム", icon: "▣" },
];
