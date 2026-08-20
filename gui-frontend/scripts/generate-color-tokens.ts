/**
 * Material 3 の色ロールを生成して src/styles/color-tokens.css を書き出す。
 *
 * spec §1-2 の「面は無彩色、アクセントだけ色を持つ」を、neutral / neutralVariant の
 * chroma を 0 にしたカスタム DynamicScheme で実現する。M3 の標準スキーム
 * （tonal spot）は surface にも source color の色相が薄く乗り、写真ツールでは
 * 背景の色被りが色判断を狂わせるため採らない。
 *
 * 実行: cd gui-frontend && bun run gen:colors
 */
import {
  DynamicScheme,
  Hct,
  MaterialDynamicColors,
  TonalPalette,
  Variant,
  argbFromHex,
  hexFromArgb,
} from "@material/material-color-utilities";

/**
 * source color は現行 app.css の --accent-hover。
 * 現行の --accent (#818cf8) は同色相で明るいトーンであり、生成後は primary の
 * 明るいトーンとして再現される（spec §1-2）。
 */
const SOURCE_HEX = "#6366F1";

/** spec §1-1 の表で「使う」と決めた 21 ロール。これ以外は定義しない。 */
const ROLES: [string, { getArgb(scheme: DynamicScheme): number }][] = [
  ["primary", MaterialDynamicColors.primary],
  ["on-primary", MaterialDynamicColors.onPrimary],
  ["primary-container", MaterialDynamicColors.primaryContainer],
  ["on-primary-container", MaterialDynamicColors.onPrimaryContainer],
  ["surface", MaterialDynamicColors.surface],
  ["surface-container-lowest", MaterialDynamicColors.surfaceContainerLowest],
  ["surface-container-low", MaterialDynamicColors.surfaceContainerLow],
  ["surface-container", MaterialDynamicColors.surfaceContainer],
  ["surface-container-high", MaterialDynamicColors.surfaceContainerHigh],
  ["surface-container-highest", MaterialDynamicColors.surfaceContainerHighest],
  ["on-surface", MaterialDynamicColors.onSurface],
  ["on-surface-variant", MaterialDynamicColors.onSurfaceVariant],
  ["outline", MaterialDynamicColors.outline],
  ["outline-variant", MaterialDynamicColors.outlineVariant],
  ["error", MaterialDynamicColors.error],
  ["on-error", MaterialDynamicColors.onError],
  ["error-container", MaterialDynamicColors.errorContainer],
  ["on-error-container", MaterialDynamicColors.onErrorContainer],
  ["inverse-surface", MaterialDynamicColors.inverseSurface],
  ["inverse-on-surface", MaterialDynamicColors.inverseOnSurface],
  ["scrim", MaterialDynamicColors.scrim],
];

function buildScheme(isDark: boolean): DynamicScheme {
  const source = Hct.fromInt(argbFromHex(SOURCE_HEX));
  return new DynamicScheme({
    sourceColorHct: source,
    variant: Variant.TONAL_SPOT,
    contrastLevel: 0,
    isDark,
    // primary だけ source の chroma を保つ
    primaryPalette: TonalPalette.fromHueAndChroma(source.hue, source.chroma),
    // secondary / tertiary は spec §1-1 で「定義しない」と決めたロールにしか
    // 使われないが、DynamicScheme は必ず全パレットを持つ。無彩色にしておけば
    // 誤って参照しても色が漏れない。
    secondaryPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    tertiaryPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    // 面と線は完全な無彩色（spec §1-2）
    neutralPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    neutralVariantPalette: TonalPalette.fromHueAndChroma(source.hue, 0),
    // errorPalette は既定（M3 標準の赤）のまま。状態色まで無彩色にすると
    // 「危険」が伝わらない。
  });
}

function block(isDark: boolean): string {
  const scheme = buildScheme(isDark);
  const prefix = isDark ? "dark" : "light";
  return ROLES.map(
    ([name, color]) => `  --_${prefix}-${name}: ${hexFromArgb(color.getArgb(scheme))};`
  ).join("\n");
}

const css = `/* 生成物。編集しないこと。
 * 再生成: cd gui-frontend && bun run gen:colors
 * 生成元: scripts/generate-color-tokens.ts（source color ${SOURCE_HEX}）
 *
 * ここは値の置き場でしかない。--md-sys-color-* への割り当ては tokens.css が行う。
 */
:root {
  /* ライト */
${block(false)}

  /* ダーク */
${block(true)}
}
`;

await Bun.write(new URL("../src/styles/color-tokens.css", import.meta.url), css);
console.log("wrote src/styles/color-tokens.css");
