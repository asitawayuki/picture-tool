//! 引数から「実際にどうするか」を決める部分。
//!
//! `main` に埋め込むと、モード制限・丸め・プリセットの解決といった**分岐のある判断**が
//! 一切テストできない（引数解析から先が 1 本の手続きになる）。ここには I/O を持ち込まず、
//! 入力から出力を決めるだけにしてある。警告文は返り値で渡し、stderr へ出すのは `main`。

use picture_tool_core::exif_frame::ExifFrameConfig;
use picture_tool_core::{ConversionMode, ProcessResult};

/// `--max-width` について利用者に伝えるべきこと。
///
/// 上限は crop / pad でしか効かず、実効値は 4 の倍数へ切り捨てられる。
/// どちらも黙って起きると「指定したのに効いていない」に見えるため文言を返す。
pub fn max_width_warning(mode: ConversionMode, max_width: Option<u32>) -> Option<String> {
    let max_width = max_width?;
    if mode == ConversionMode::Quality {
        return Some(
            "--max-width is only supported with --mode crop or pad. Ignoring.".to_string(),
        );
    }
    // 切り上げると指定値を超えてしまうので必ず切り捨てる
    let effective = max_width / 4 * 4;
    if effective == max_width {
        return None;
    }
    Some(format!(
        "--max-width {} is rounded down to {} \
         (the output canvas width is always a multiple of 4).",
        max_width, effective
    ))
}

/// Exif フレームを実際に付けるか。付けない場合は理由を添える。
pub fn resolve_exif_frame(requested: bool, mode: ConversionMode) -> (bool, Option<String>) {
    if !requested {
        return (false, None);
    }
    if mode == ConversionMode::Pad {
        return (true, None);
    }
    // pad 以外には帯を置く余白が無い
    (
        false,
        Some("--exif-frame is only supported with --mode pad. Ignoring.".to_string()),
    )
}

/// 名前でプリセットを選ぶ。見つからなければ既定に落として理由を返す。
pub fn select_preset(
    presets: Vec<ExifFrameConfig>,
    name: &str,
) -> (ExifFrameConfig, Option<String>) {
    match presets.into_iter().find(|p| p.name == name) {
        Some(found) => (found, None),
        None => (
            ExifFrameConfig::default(),
            Some(format!("Preset '{}' not found, using default", name)),
        ),
    }
}

/// `--custom-text` でプリセットのテキストを上書きする。
pub fn apply_custom_text(config: &mut ExifFrameConfig, custom_text: &str) {
    if custom_text.is_empty() {
        // 既定値も空文字なので、ここで抜けないと「指定しなかった」がプリセットの
        // テキストを消してしまう
        return;
    }
    config.custom_text = custom_text.to_string();
    // 項目が off のままだと、指定したテキストがどこにも出ない
    config.items.custom_text = true;
}

/// バッチの結果の内訳
#[derive(Debug, PartialEq, Eq)]
pub struct BatchSummary {
    pub succeeded: usize,
    pub failed: usize,
    /// 下限品質でも `--max-size` に収まらなかった枚数。**失敗ではない**（出力はできている）
    pub over_limit: usize,
}

impl BatchSummary {
    /// 1 件でも変換できなければ非 0。スクリプトから失敗を検出できるようにする。
    pub fn exit_code(&self) -> std::process::ExitCode {
        if self.failed > 0 {
            std::process::ExitCode::FAILURE
        } else {
            std::process::ExitCode::SUCCESS
        }
    }
}

pub fn summarize(results: &[anyhow::Result<ProcessResult>]) -> BatchSummary {
    let mut summary = BatchSummary {
        succeeded: 0,
        failed: 0,
        over_limit: 0,
    };
    for result in results {
        match result {
            Ok(r) => {
                summary.succeeded += 1;
                if r.size_limit_exceeded {
                    summary.over_limit += 1;
                }
            }
            Err(_) => summary.failed += 1,
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use picture_tool_core::exif_frame::{DisplayItems, ExifPosition, FontConfig};

    fn preset(name: &str) -> ExifFrameConfig {
        ExifFrameConfig {
            name: name.to_string(),
            position: ExifPosition::Auto,
            items: DisplayItems::default(),
            font: FontConfig::default(),
            custom_text: String::new(),
        }
    }

    fn ok(size_limit_exceeded: bool) -> anyhow::Result<ProcessResult> {
        Ok(ProcessResult {
            input_path: "/in/a.jpg".to_string(),
            output_path: "/out/a.jpg".to_string(),
            final_size_mb: 1.0,
            final_quality: Some(90),
            size_limit_exceeded,
            warnings: Vec::new(),
        })
    }

    // --- --max-width: crop / pad 限定、実効値は 4 の倍数へ切り捨て ---

    #[test]
    fn max_width_is_reported_as_ignored_in_quality_mode() {
        // quality は 4:5 に変換しないので「幅」で長辺を縛れない。
        // 黙って無視すると「指定したのに効かない」に見える
        let warning = max_width_warning(ConversionMode::Quality, Some(1000))
            .expect("quality モードでは無視することを伝える");
        assert!(warning.contains("crop"), "使えるモードを示す: {warning}");
        assert!(warning.contains("pad"), "使えるモードを示す: {warning}");
    }

    #[test]
    fn max_width_reports_the_value_it_was_rounded_down_to() {
        let warning = max_width_warning(ConversionMode::Crop, Some(1002))
            .expect("切り捨てが起きたことを伝える");
        assert!(warning.contains("1002"), "指定値を示す: {warning}");
        assert!(warning.contains("1000"), "実効値を示す: {warning}");
    }

    #[test]
    fn max_width_says_nothing_when_it_is_already_a_multiple_of_four() {
        assert_eq!(max_width_warning(ConversionMode::Crop, Some(1000)), None);
    }

    #[test]
    fn max_width_rounds_down_at_the_smallest_accepted_value() {
        // 下限は 4。5 は 4 へ落ちる（切り上げると指定値を超えてしまう）
        let warning = max_width_warning(ConversionMode::Pad, Some(5)).expect("5 は 4 へ落ちる");
        assert!(warning.contains('4'), "実効値を示す: {warning}");
    }

    #[test]
    fn max_width_says_nothing_when_it_was_not_given() {
        // 指定していないモードの組み合わせで「無視した」と言わない
        assert_eq!(max_width_warning(ConversionMode::Quality, None), None);
        assert_eq!(max_width_warning(ConversionMode::Crop, None), None);
    }

    // --- Exif フレーム: pad 限定 ---

    #[test]
    fn exif_frame_is_applied_only_in_pad_mode() {
        let (enabled, warning) = resolve_exif_frame(true, ConversionMode::Pad);
        assert!(enabled);
        assert_eq!(warning, None);
    }

    #[test]
    fn exif_frame_is_dropped_with_a_reason_outside_pad_mode() {
        for mode in [ConversionMode::Crop, ConversionMode::Quality] {
            let (enabled, warning) = resolve_exif_frame(true, mode);
            assert!(!enabled, "{mode:?} では余白が無いので描けない");
            let warning = warning.expect("落としたことを伝える");
            assert!(warning.contains("pad"), "使えるモードを示す: {warning}");
        }
    }

    #[test]
    fn exif_frame_says_nothing_when_it_was_not_requested() {
        assert_eq!(
            resolve_exif_frame(false, ConversionMode::Crop),
            (false, None)
        );
    }

    // --- プリセットの選択 ---

    #[test]
    fn select_preset_returns_the_one_that_matches_the_name() {
        let (config, warning) = select_preset(vec![preset("default"), preset("film")], "film");
        assert_eq!(config.name, "film");
        assert_eq!(warning, None);
    }

    #[test]
    fn select_preset_falls_back_to_the_default_and_says_which_name_was_missing() {
        let (config, warning) = select_preset(vec![preset("default")], "nope");
        assert_eq!(config.name, ExifFrameConfig::default().name);
        let warning = warning.expect("要求した名前が無かったことを伝える");
        assert!(warning.contains("nope"), "要求された名前を示す: {warning}");
    }

    #[test]
    fn select_preset_falls_back_when_there_are_no_presets_at_all() {
        let (config, warning) = select_preset(Vec::new(), "default");
        assert_eq!(config.name, ExifFrameConfig::default().name);
        assert!(warning.is_some());
    }

    // --- --custom-text ---

    #[test]
    fn custom_text_overrides_the_preset_and_turns_the_item_on() {
        // 上書きしても表示項目が off のままだと、指定したテキストがどこにも出ない
        let mut config = preset("film");
        config.custom_text = "old".to_string();
        config.items.custom_text = false;

        apply_custom_text(&mut config, "Shot on film");

        assert_eq!(config.custom_text, "Shot on film");
        assert!(config.items.custom_text);
    }

    #[test]
    fn custom_text_leaves_the_preset_alone_when_it_was_not_given() {
        // 既定値は空文字なので、「指定しなかった」を上書きと区別する必要がある
        let mut config = preset("film");
        config.custom_text = "from preset".to_string();
        config.items.custom_text = true;

        apply_custom_text(&mut config, "");

        assert_eq!(config.custom_text, "from preset");
        assert!(config.items.custom_text);
    }

    // --- 結果の集計と終了コード ---

    #[test]
    fn a_batch_without_failures_exits_zero() {
        let summary = summarize(&[ok(false), ok(false)]);
        assert_eq!(
            summary,
            BatchSummary {
                succeeded: 2,
                failed: 0,
                over_limit: 0
            }
        );
        assert_eq!(summary.exit_code(), std::process::ExitCode::SUCCESS);
    }

    #[test]
    fn a_single_failure_makes_the_whole_run_exit_non_zero() {
        // 呼び出し側（シェルスクリプト）が失敗を検出できるようにする
        let summary = summarize(&[ok(false), Err(anyhow::anyhow!("Failed to open image"))]);
        assert_eq!(
            summary,
            BatchSummary {
                succeeded: 1,
                failed: 1,
                over_limit: 0
            }
        );
        assert_eq!(summary.exit_code(), std::process::ExitCode::FAILURE);
    }

    #[test]
    fn exceeding_the_size_limit_is_counted_but_is_not_a_failure() {
        // 下限品質でも収まらなかった場合でも出力はできている。
        // 変換できなかった訳ではないので終了コードは 0 のまま
        let summary = summarize(&[ok(true)]);
        assert_eq!(
            summary,
            BatchSummary {
                succeeded: 1,
                failed: 0,
                over_limit: 1
            }
        );
        assert_eq!(summary.exit_code(), std::process::ExitCode::SUCCESS);
    }

    #[test]
    fn an_empty_batch_exits_zero() {
        let summary = summarize(&[]);
        assert_eq!(
            summary,
            BatchSummary {
                succeeded: 0,
                failed: 0,
                over_limit: 0
            }
        );
        assert_eq!(summary.exit_code(), std::process::ExitCode::SUCCESS);
    }
}
