//! 画像ファイルのデコード（`image` クレートが読めない JPEG XL / AVIF の振り分け）
//!
//! 形式は**拡張子ではなく中身で判定する**。JPEG XL / AVIF は先頭バイトで見分け、
//! それ以外も `image` の中身による推定を優先する（拡張子は推定できないときの手がかり）。
//!
//! # 向き（Orientation）
//!
//! JPEG XL はコードストリームの向き、AVIF は `irot`/`imir` が正で、どちらの仕様も
//! Exif の Orientation を無視するよう定めている。この 2 形式は [`open`] / [`dimensions`] が
//! 向きを適用済みで返すので、[`Format::exif_orientation_applies`] が偽の形式では
//! Exif の向きを使わない（使うと二重回転になる）。
//!
//! # メモリの上限
//!
//! **出力の画素バッファは `image::open` と同じ `Limits::default()`（512MB）までにする。**
//! 寸法だけ巨大に偽装したファイルで OOM になると abort で `catch_unwind` でも止まらないため、
//! 画素を確保する前に宣言された寸法で弾く。
//!
//! **512MB の判定は [`probe`]（ヘッダだけを読む）で行い、[`open`] では行わない。** メモリ予算
//! （spec 2026-10-28）は「probe → 見積もり → 借りる → open」の順で使うので、512MB を超えるものは
//! 見積もりの前に弾かれる（メモリの少ない PC で単独処理の待ちに入ってから失敗させない）。[`open`] は
//! デコーダが読んだ寸法・色型が [`Header`] と同じことを確かめるので、probe の判定が open にも効く。
//!
//! デコーダ内部の作業領域はこの 512MB に含めない（JPEG XL は 1 画素あたり出力の数倍を使い、
//! 含めると 20MP 前後の普通の写真が開けなくなる）。作業領域とファイルの大きさは、メモリ予算の
//! 見積もり（`budget::estimate`）に入れる:
//! - **JPEG XL**: jxl-oxide の作業領域の上限（`max_alloc`）に、見積もりの作業領域の項
//!   （`budget::estimate::jxl_working_bytes`）をそのまま渡す。作業領域は画像ヘッダではなくフレームの
//!   寸法で確保され、フレームは画像より大きく宣言できる（`have_crop`）ので、上限が無いと画像 1x1・
//!   フレーム巨大の数 KB のファイルで abort する
//!
//! AVIF は 2 段で判定する:
//! 1. **デコード前**: AV1 のシーケンスヘッダが宣言する寸法（主画像とアルファのアイテムごと）から
//!    出力の大きさを見積もり、512MB を超えれば弾く（[`frame_size_limits`]）。通過したら、
//!    その宣言寸法を `frame_size_limit` として libdav1d に強制させる。宣言より大きい
//!    フレームはデコーダが確保前に拒否するので、宣言が上限の根拠になる
//! 2. **変換前**: 実際にデコードされた画像の大きさで、YUV→RGB 変換の出力が 512MB を
//!    超えないか再度確かめる（`yuv.rs`）
//!
//! libdav1d 内部の作業領域（ピクチャ本体・フィルムグレインの複製・参照フレーム）は
//! 「宣言寸法 × 深度 × 参照フレーム数」まで使いうる。この 512MB には含めず、メモリ予算の見積もりに
//! 「宣言寸法 × 3 平面 × 深度 × 10 枚」と、ファイル全体の読み込みとアイテムの複製（ファイルの大きさの 2 倍）を入れる。

mod av1;
mod avif;
mod isobmff;
mod jxl;
mod yuv;

use anyhow::{Context, Result, anyhow, ensure};
use image::{ColorType, DynamicImage, ImageDecoder as _, ImageReader};
use jxl_oxide::integration::JxlDecoder;
use std::fs::File;
use std::io::{BufReader, Cursor, Read};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Jxl,
    Avif,
    /// `image` クレートに任せるもの（JPEG / PNG / WebP）
    Other,
}

impl Format {
    /// Exif の Orientation を画素に適用すべき形式か
    pub(crate) fn exif_orientation_applies(self) -> bool {
        self == Format::Other
    }
}

/// 先頭バイトから形式を判定する。読めない・短いファイルは `Other`（後段の `image` が報告する）
pub(crate) fn sniff(path: &Path) -> Result<Format> {
    let mut head = Vec::with_capacity(64);
    File::open(path)
        .with_context(|| format!("Failed to open image: {}", path.display()))?
        .take(64)
        .read_to_end(&mut head)
        .with_context(|| format!("Failed to read image: {}", path.display()))?;
    Ok(sniff_bytes(&head))
}

const JXL_CODESTREAM: [u8; 2] = [0xFF, 0x0A];

fn sniff_bytes(head: &[u8]) -> Format {
    const JXL_CONTAINER: &[u8] = b"\0\0\0\x0CJXL \r\n\x87\n";
    if head.starts_with(&JXL_CODESTREAM) || head.starts_with(JXL_CONTAINER) {
        return Format::Jxl;
    }
    // ISOBMFF の ftyp: [size:4][b"ftyp"][major:4]...。avif-parse は major brand が
    // `avif` のものしか読まないので、互換ブランドだけに avif を持つものは AVIF として扱わない。
    // `avis`（アニメーション）は AVIF に回し、非対応である旨を avif-parse に報告させる
    if matches!(head.get(4..12), Some(b"ftypavif" | b"ftypavis")) {
        return Format::Avif;
    }
    Format::Other
}

/// ヘッダだけから読んだ 1 枚の素性（[`probe`]）。メモリ予算の見積もり（`budget::estimate`）の入力で、
/// デコード（[`open`]）はこれと同じ寸法・色型であることを確かめてから画素を読む
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) format: Format,
    /// JPEG XL・AVIF は向き適用後、他は生の寸法（[`dimensions`] と同じ意味）
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// 出力の色型。AVIF は主画像の深度とアルファの有無から決めた上限（4:0:0 でも RGB とみなす）
    pub(crate) color: ColorType,
    pub(crate) file_len: u64,
    pub(crate) avif: Option<AvifDeclared>,
}

/// AVIF のシーケンスヘッダの宣言（主画像とアルファ）。デコーダの `frame_size_limit` の根拠
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AvifDeclared {
    pub(crate) primary_pixels: u64,
    pub(crate) alpha_pixels: Option<u64>,
    pub(crate) bit_depth: u8,
}

const CHANGED: &str = "image changed between probing and decoding";

/// 画素を読まずに 1 枚の素性を得る。**出力 512MB の上限の判定もここで済ませる**
///
/// 見積もり → 予算を借りる → デコード、の順で使う。512MB を超えるものを見積もりの前に弾くのは、
/// メモリの少ない PC で「単独処理」の待ち（全員を止める）に入ってから失敗させないため。
/// 寸法が読めなければ `Err`（fail-closed。見積もりを経ずにデコードへ入る経路を作らない）
pub(crate) fn probe(path: &Path) -> Result<Header> {
    let format = sniff(path)?;
    guarded(|| match format {
        Format::Jxl => {
            let decoder = jxl_decoder(path)?;
            image::Limits::default().reserve(decoder.total_bytes())?;
            let (width, height) = decoder.dimensions();
            Ok(Header {
                format,
                width,
                height,
                color: decoder.color_type(),
                file_len: std::fs::metadata(path)?.len(),
                avif: None,
            })
        }
        // ファイル全体を読むのは、宣言がアイテムのデータ（シーケンスヘッダ）にあるため。
        // 読んだバイト列はデコードへ持ち越さない: 予算を待つワーカーが予算外のバイト列を抱えて並ぶことになる
        Format::Avif => {
            let data = std::fs::read(path)?;
            let layout = avif::read_layout(Cursor::new(&data))?;
            let (primary, alpha) = avif_items(&data)?;
            frame_size_limits(primary, alpha)?;
            let (width, height) = layout.oriented_dimensions();
            let color = match (primary.bit_depth > 8, alpha.is_some()) {
                (false, false) => ColorType::Rgb8,
                (false, true) => ColorType::Rgba8,
                (true, false) => ColorType::Rgb16,
                (true, true) => ColorType::Rgba16,
            };
            Ok(Header {
                format,
                width,
                height,
                color,
                file_len: data.len() as u64,
                avif: Some(AvifDeclared {
                    primary_pixels: primary.pixels,
                    alpha_pixels: alpha.map(|a| a.pixels),
                    bit_depth: primary.bit_depth,
                }),
            })
        }
        Format::Other => {
            let decoder = ImageReader::open(path)?
                .with_guessed_format()?
                .into_decoder()?;
            image::Limits::default().reserve(decoder.total_bytes())?;
            let (width, height) = decoder.dimensions();
            Ok(Header {
                format,
                width,
                height,
                color: decoder.color_type(),
                file_len: std::fs::metadata(path)?.len(),
                avif: None,
            })
        }
    })
    .with_context(|| format!("Failed to open image: {}", path.display()))
}

/// 画像をデコードする。JPEG XL / AVIF は形式固有の向きを適用済み、それ以外は向き未適用
///
/// [`open_with_warnings`] の警告を捨てる包み（サムネイル・プレビュー用）。
pub(crate) fn open(path: &Path, header: &Header) -> Result<DynamicImage> {
    open_with_warnings(path, header).map(|(img, _)| img)
}

/// [`open`] と同じくデコードし、ICC の扱いで起きた事象を警告として返す
///
/// JPEG / PNG / WebP は埋め込まれた ICC を sRGB へ変換する（[`crate::color::to_srgb`]）。
/// ICC の失敗は警告にして続け、`Err` にしない（spec 2026-10-20「付随処理の失敗」）。
/// JPEG XL / AVIF は色管理をしない（spec「やらないこと」）ので警告は常に空。
///
/// **出力 512MB の上限は [`probe`] が判定済みで、ここでは判定しない。** 代わりに、デコーダが読んだ
/// 寸法・色型（AVIF は宣言とファイルの大きさ）が `header` と同じことを確かめる。ヘッダを 2 回読むので、
/// その間にファイルが差し替わると、見積もりと無関係な大きさを開くことになるため
pub(crate) fn open_with_warnings(
    path: &Path,
    header: &Header,
) -> Result<(DynamicImage, Vec<String>)> {
    let mut warnings = Vec::new();
    let img = match header.format {
        Format::Jxl => guarded(|| {
            let mut decoder = jxl_decoder(path)?;
            ensure!(
                decoder.dimensions() == (header.width, header.height)
                    && decoder.color_type() == header.color,
                CHANGED
            );
            // 作業領域の上限は見積もりの作業領域の項そのもの。デコードを始める前に設定すれば効く
            let mut limits = image::Limits::no_limits();
            limits.max_alloc = Some(crate::budget::estimate::jxl_working_bytes(header));
            decoder.set_limits(limits)?;
            Ok(DynamicImage::from_decoder(decoder)?)
        }),
        Format::Avif => guarded(|| {
            let data = std::fs::read(path)?;
            let layout = avif::read_layout(Cursor::new(&data))?;
            let parsed = avif_parse::read_avif(&mut data.as_slice())?;
            let (primary, alpha) = declared_items(&parsed)?;
            ensure!(
                data.len() as u64 == header.file_len
                    && layout.oriented_dimensions() == (header.width, header.height)
                    && header.avif
                        == Some(AvifDeclared {
                            primary_pixels: primary.pixels,
                            alpha_pixels: alpha.map(|a| a.pixels),
                            bit_depth: primary.bit_depth,
                        }),
                CHANGED
            );
            let limits = frame_size_limits(primary, alpha)?;
            let color = av1::decode(&parsed.primary_item, limits.primary)?;
            let alpha = match (&parsed.alpha_item, limits.alpha) {
                (Some(a), Some(limit)) => Some(av1::decode(a, limit)?),
                _ => None,
            };
            Ok(layout.apply(yuv::to_image(
                &color,
                alpha.as_ref(),
                parsed.premultiplied_alpha,
            )?))
        }),
        // ICC の解析・変換（moxcms）は信頼できない入力に対する新しい攻撃面なので、JPEG XL / AVIF と同じく
        // panic を 1 枚の `Err` にする（解析と変換の作成の panic は `to_srgb` が警告にする。ここは保険）
        Format::Other => guarded(|| {
            let mut decoder = ImageReader::open(path)?
                .with_guessed_format()?
                .into_decoder()?;
            ensure!(
                decoder.dimensions() == (header.width, header.height)
                    && decoder.color_type() == header.color,
                CHANGED
            );
            let icc = decoder.icc_profile().unwrap_or_else(|e| {
                warnings.push(format!(
                    "Ignoring unreadable ICC profile in {}: {e}",
                    path.display()
                ));
                None
            });
            let img = DynamicImage::from_decoder(decoder)?;
            Ok(crate::color::to_srgb(
                img,
                icc.as_deref(),
                path,
                &mut warnings,
            ))
        }),
    }
    .with_context(|| format!("Failed to open image: {}", path.display()))?;
    Ok((img, warnings))
}

/// 画素を読まずに (幅, 高さ) を得る。JPEG XL / AVIF は向き適用後の寸法
///
/// 一覧表示で全ファイルに対して呼ばれるので、ヘッダより先は読まない。
pub(crate) fn dimensions(path: &Path, format: Format) -> Result<(u32, u32)> {
    match format {
        Format::Jxl => guarded(|| Ok(jxl_decoder(path)?.dimensions())),
        Format::Avif => guarded(|| {
            Ok(avif::read_layout(BufReader::new(File::open(path)?))?.oriented_dimensions())
        }),
        Format::Other => Ok(ImageReader::open(path)?
            .with_guessed_format()?
            .into_dimensions()?),
    }
    .with_context(|| format!("Failed to read image dimensions: {}", path.display()))
}

/// JPEG XL の Exif ボックスを `exif` クレートで読める生 TIFF として取り出す。
/// 無い（または上限を超えて大きい）ときは `Ok(None)`、箱が壊れているときは `Err`
pub(crate) fn jxl_exif(path: &Path) -> Result<Option<Vec<u8>>> {
    guarded(|| jxl::raw_exif(BufReader::new(File::open(path)?)))
}

/// [`jxl_exif`] の `Err` を「無い」に畳んだもの（撮影情報の表示用。壊れていても黙って無いものとする）
pub(crate) fn jxl_raw_exif(path: &Path) -> Option<Vec<u8>> {
    jxl_exif(path).ok().flatten()
}

/// コードストリームだけを渡した jxl-oxide のデコーダ（ヘッダまで読んだ状態）
///
/// 剥がした中身が素のコードストリームで始まることを確かめる。コンテナの中にコンテナを
/// 入れられると、jxl-oxide が内側の `brob` を上限なしで展開してしまう（`jxl` モジュール冒頭）。
///
/// **スレッドプールは使わない（`JxlThreadPool::none()`）。** jxl-oxide は参照フレームの
/// 描画を rayon のプールへ切り離して投げる。そこで panic すると rayon がプロセスを
/// abort させ、`catch_unwind` が効かない。プールなしならその場で実行されるので捕まえられる。
/// 一括変換はファイル単位で並列に動くので全体は遅くならない（1 枚だけを開くときは、
/// 24MP の非可逆 JPEG XL で約 0.5 秒遅くなる）。
fn jxl_decoder(path: &Path) -> Result<JxlDecoder<impl Read>> {
    let mut codestream = jxl::Codestream::new(BufReader::new(File::open(path)?))?;
    let mut signature = [0u8; 2];
    codestream.read_exact(&mut signature)?;
    ensure!(
        signature == JXL_CODESTREAM,
        "JPEG XL container does not hold a bare codestream"
    );
    Ok(JxlDecoder::with_thread_pool(
        Cursor::new(signature).chain(codestream),
        jxl_oxide::JxlThreadPool::none(),
    )?)
}

/// AVIF の主画像とアルファのアイテムが宣言する寸法と深度
fn declared_items(
    parsed: &avif_parse::AvifData,
) -> Result<(avif::Declared, Option<avif::Declared>)> {
    Ok((
        avif::declared(&parsed.primary_item)?,
        parsed
            .alpha_item
            .as_deref()
            .map(avif::declared)
            .transpose()?,
    ))
}

fn avif_items(data: &[u8]) -> Result<(avif::Declared, Option<avif::Declared>)> {
    declared_items(&avif_parse::read_avif(&mut &data[..])?)
}

/// AVIF のデコーダに渡す `frame_size_limit`（画素数）。主画像とアルファで別々に持つ
#[derive(Debug)]
struct FrameSizeLimits {
    primary: u32,
    alpha: Option<u32>,
}

/// AVIF のデコードに先立って、宣言された寸法で弾き、デコーダに渡す `frame_size_limit` を決める
///
/// 判定は出力の画素バッファ（主画像 RGB ＋ アルファ 1 チャンネル）の合計を 1 つの
/// `Limits::default()`（512MB）に数える。理由:
/// - シーケンスヘッダの数は [`avif::declared`] が 1 に限っている。宣言の最大寸法より大きい
///   フレームは `frame_size_limit` でデコーダが拒否するので、宣言が上限の根拠になる
/// - アルファの深度は主画像のものを使う。変換後の出力は主画像の深度で作られるため
/// - `frame_size_limit` は 0 が無制限なので、`u32` に収まらない値や 0 を渡す前に、
///   必ず 512MB 判定を先に通す（`u32::try_from` はその後）。判定を通れば 2^32 画素未満である
/// - アルファには `min(アルファの宣言, 主画像の宣言)` を渡す。アルファは主画像と同じ寸法で
///   なければならず、主画像より大きい宣言を許す理由がない
fn frame_size_limits(
    primary: avif::Declared,
    alpha: Option<avif::Declared>,
) -> Result<FrameSizeLimits> {
    let bytes_per_sample: u64 = if primary.bit_depth > 8 { 2 } else { 1 };
    let mut budget = image::Limits::default();
    let over =
        |e| anyhow::Error::new(e).context("AVIF declares a size over the 512MB output limit");
    budget
        .reserve(primary.pixels.saturating_mul(3 * bytes_per_sample))
        .map_err(over)?;
    if let Some(alpha) = &alpha {
        budget
            .reserve(alpha.pixels.saturating_mul(bytes_per_sample))
            .map_err(over)?;
    }
    let limit = |pixels: u64| u32::try_from(pixels).context("AVIF declares too many pixels");
    Ok(FrameSizeLimits {
        primary: limit(primary.pixels)?,
        alpha: alpha
            .map(|a| limit(a.pixels.min(primary.pixels)))
            .transpose()?,
    })
}

/// JPEG XL / AVIF のデコーダを panic から隔離する
///
/// 外部デコーダは壊れた入力で panic しうる（例: avif-parse は途中で切れたファイルで
/// `debug_assert!` に落ちる）。core が panic すると GUI ごと落ちるので、1 枚の失敗として
/// 扱えるよう `Err` に変える。`metadata::write_metadata` と同じ方針。
///
/// **捕まえられるのは呼び出したスレッドで起きた panic だけ。** そのため jxl-oxide には
/// スレッドプールを使わせない（[`jxl_decoder`]）。libdav1d（AVIF）のワーカーは C のスレッドで
/// Rust の panic は起きない（`n_threads = 1` でワーカーも起こさない）。dav1d クレート側の
/// panic（予約値の `unreachable!()` 等）は呼び出したスレッドで起き、ここで捕まえる。
fn guarded<T>(f: impl FnOnce() -> Result<T>) -> Result<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|_| Err(anyhow!("decoder panicked on malformed input")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_recognises_signatures() {
        assert_eq!(sniff_bytes(&[0xFF, 0x0A, 0x00]), Format::Jxl);
        assert_eq!(sniff_bytes(b"\0\0\0\x0CJXL \r\n\x87\n\0\0"), Format::Jxl);
        assert_eq!(sniff_bytes(b"\0\0\0\x14ftypavif\0\0\0\0mif1"), Format::Avif);
        assert_eq!(sniff_bytes(b"\0\0\0\x14ftypavis\0\0\0\0mif1"), Format::Avif);
        // 互換ブランドにしか avif がないものは、デコーダ（avif-parse）が読めないので AVIF にしない
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypmif1\0\0\0\0miafavif"),
            Format::Other
        );
        // HEIC は AVIF ではない
        assert_eq!(
            sniff_bytes(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            Format::Other
        );
        assert_eq!(sniff_bytes(&[0xFF, 0xD8, 0xFF]), Format::Other);
        assert_eq!(sniff_bytes(&[]), Format::Other);
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbImage::from_pixel(width, height, image::Rgb([128, 96, 64]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn decode_rejects_a_file_changed_after_probe() {
        let dir = tempfile::tempdir().unwrap();
        let png_a = dir.path().join("a.png");
        let png_b = dir.path().join("b.png");
        write_png(&png_a, 64, 64);
        write_png(&png_b, 32, 32);
        // 形式ごとに (A, 寸法の違う B)
        let pairs = [
            (fixture("rotated.jxl"), fixture("lossy_24mp.jxl"), "a.jxl"),
            (
                fixture("orient_1.avif"),
                fixture("avif/p8_420_lim_unspec.avif"),
                "a.avif",
            ),
            (png_a, png_b, "a2.png"),
        ];
        for (a, b, name) in pairs {
            // 前提: B は単独なら開ける
            open(&b, &probe(&b).unwrap()).unwrap();
            let path = dir.path().join(name);
            std::fs::copy(&a, &path).unwrap();
            let header = probe(&path).unwrap();
            std::fs::copy(&b, &path).unwrap();
            let err = open(&path, &header).unwrap_err();
            assert!(
                format!("{err:#}").contains("image changed between probing and decoding"),
                "{name}: {err:#}"
            );
        }
    }

    #[test]
    fn over_output_limit_is_rejected_by_probe_before_borrowing() {
        for name in ["over_limit.jxl", "over_limit.png", "avif/over_limit.avif"] {
            let path = fixture(name);
            // 前提: 寸法はヘッダから読める（壊れたファイルとして落ちているのではない）
            assert!(dimensions(&path, sniff(&path).unwrap()).is_ok(), "{name}");
            assert!(probe(&path).is_err(), "{name}");
        }
        let err = probe(&fixture("avif/over_limit.avif")).unwrap_err();
        assert!(
            format!("{err:#}").contains("AVIF declares a size over the 512MB output limit"),
            "{err:#}"
        );
    }

    #[test]
    fn unreadable_dimensions_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let garbage = dir.path().join("garbage.jpg");
        std::fs::write(&garbage, b"this is not an image at all").unwrap();
        let empty = dir.path().join("empty.jxl");
        std::fs::write(&empty, b"").unwrap();
        // ftyp の途中で切った AVIF（meta も mdat も無い）
        let avif = std::fs::read(fixture("orient_1.avif")).unwrap();
        let cut = dir.path().join("cut.avif");
        std::fs::write(&cut, &avif[..16]).unwrap();
        for path in [garbage, empty, cut] {
            assert!(probe(&path).is_err(), "{}", path.display());
        }
    }

    #[test]
    fn header_only_files_pass_probe_and_fail_to_decode() {
        // ヘッダが読める壊れたファイルを、見積もりの前に落とさない（デコードで 1 枚の Err になる）
        let dir = tempfile::tempdir().unwrap();
        let jxl = std::fs::read(fixture("lossy_24mp.jxl")).unwrap();
        let head = dir.path().join("head.jxl");
        std::fs::write(&head, &jxl[..200]).unwrap();
        // crash.avif はタイルデータの中を壊したもの（宣言は読める）
        for path in [head, fixture("avif/crash.avif")] {
            let header = probe(&path).unwrap();
            assert!(open(&path, &header).is_err(), "{}", path.display());
        }
    }

    fn d(pixels: u64, bit_depth: u8) -> avif::Declared {
        avif::Declared { pixels, bit_depth }
    }

    #[test]
    fn frame_size_limits_passes_the_declared_sizes() {
        let l = frame_size_limits(d(2048, 8), None).unwrap();
        assert_eq!((l.primary, l.alpha), (2048, None));
        // アルファは主画像との小さいほう
        let l = frame_size_limits(d(4096, 8), Some(d(1_000_000, 8))).unwrap();
        assert_eq!(l.alpha, Some(4096));
        let l = frame_size_limits(d(4096, 8), Some(d(1024, 8))).unwrap();
        assert_eq!(l.alpha, Some(1024));
    }

    #[test]
    fn frame_size_limits_counts_alpha_and_depth_against_one_budget() {
        let over = |r: Result<FrameSizeLimits>| {
            format!("{:#}", r.unwrap_err())
                .contains("AVIF declares a size over the 512MB output limit")
        };
        // 100M 画素: 8bit なら 300MB で通り、10bit なら 600MB で落ちる
        assert!(frame_size_limits(d(100_000_000, 8), None).is_ok());
        assert!(over(frame_size_limits(d(100_000_000, 10), None)));
        // 主画像 64x64 ＋ アルファ 23200x23200（1 チャンネルでも合計が上限を超える）
        assert!(over(frame_size_limits(
            d(64 * 64, 8),
            Some(d(23200 * 23200, 8))
        )));
        // アルファは主画像の深度で数える: 主 10bit なら 2 バイト
        // 8bit は 300,012,288 バイトで通り、10bit は 600,024,576 バイトで超える
        assert!(frame_size_limits(d(64 * 64, 8), Some(d(300_000_000, 8))).is_ok());
        assert!(over(frame_size_limits(
            d(64 * 64, 10),
            Some(d(300_000_000, 8))
        )));
    }

    #[test]
    fn frame_size_limits_sums_primary_and_alpha_into_one_budget() {
        // 主画像 150M 画素 8bit は 450,000,000 バイト、アルファ 150M 画素 8bit は
        // 150,000,000 バイト。どちらも単独では上限 536,870,912 の内側で、合計 600MB だけが超える
        let primary = || d(150_000_000, 8);
        let alpha = || d(150_000_000, 8);
        // 前提: それぞれ単独では通る（これが崩れると、下の Err は別の理由で出ていることになる）
        assert!(frame_size_limits(primary(), None).is_ok());
        assert!(frame_size_limits(d(64 * 64, 8), Some(alpha())).is_ok());
        let err = frame_size_limits(primary(), Some(alpha())).unwrap_err();
        assert!(
            format!("{err:#}").contains("AVIF declares a size over the 512MB output limit"),
            "{err:#}"
        );
    }

    #[test]
    fn frame_size_limits_rejects_before_the_limit_would_wrap() {
        // 65536x65536 = 2^32。u32 に入らず、0（無制限）にもならないまま 512MB 判定で落ちる
        let err = frame_size_limits(d(1 << 32, 8), None).unwrap_err();
        assert!(
            format!("{err:#}").contains("AVIF declares a size over the 512MB output limit"),
            "{err:#}"
        );
    }
}
