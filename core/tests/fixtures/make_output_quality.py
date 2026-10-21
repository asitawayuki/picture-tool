"""tests/output_quality.rs と src/color.rs のテストが使う ICC のフィクスチャ（icc/*）と over_limit.png を作る。

使い方: uv run --no-project --with pillow python make_output_quality.py <リポジトリの core/tests/fixtures>
要件: Pillow 12、ImageMagick 7.1（`magick`）、colord の ICC（/usr/share/color/icc/colord/）、
      サンプル写真 docs/tmp/20260328-DSC05024.jpg（LrC が書き出したもの。ICC だけを取り出し、画素は使わない）。
中間ファイル（patch.png, lrc_srgb.icc）はコミットしない。

作り方（spec 2026-10-20「検証（ICC）」の監査と同じ）:
  4 色（200,60,60 / 60,160,90 / 90,110,200 / 210,170,120）を 8x8 ずつ横に並べた 32x8 のパッチに、
  **画素値を変えずに** ICC を割り当てる（`magick patch.png -profile X.icc`。プロファイルの無い画像への
  最初の -profile は変換ではなく割り当て）。

  icc/adobe.png     8bit RGB + AdobeRGB1998
  icc/adobe16.png   16bit RGBA（アルファ 128/255 = 32896/65535）+ AdobeRGB1998。
                    アルファを 255 にしない: 変換と背景色への合成の順序が結果に出るようにするため
  icc/adobe.jpg     AdobeRGB1998、-quality 100 -sampling-factor 1x1
  icc/adobe.webp    AdobeRGB1998、Pillow の lossless=True, icc_profile=...
  icc/prophoto.png  8bit RGB + ProPhotoRGB
  icc/srgb_lrc.jpg  サンプル写真の ICC（説明が "sRGB IEC61966-2.1" であることを確かめる）、-quality 100 -sampling-factor 1x1
  icc/broken_icc.png  iCCP の中身が b"not an icc profile"
  over_limit.png    12000x12000 の RGBA8、全画素 0。デコード後は 576,000,000 バイトで Limits::default()
                    （512MiB = 536,870,912）を超える。中身は正当な PNG（ヘッダだけを偽装していない）。
                    テストの実行時に作ると 576MB のバッファが要るのでコミットする

期待値の出どころ: 最後に ImageMagick で `-intent relative -profile sRGB.icc` に変換した値を表示する
（adobe16.png は変換してから白で `-flatten` した値）。テストの定数はこの出力から写した。
  adobe16.png では変換の直後に `-clamp` を挟む: Q16-HDRI の ImageMagick は色域外の値（緑のパッチの R が -110%）を
  切り詰めずに合成へ渡し、R が 0 になる。画素を整数で持つ本ツールは変換の時点で 0 に切り詰めるので、合成後は 127 が正しい
"""
import os
import subprocess
import sys
import tempfile

from PIL import Image

ICC = "/usr/share/color/icc/colord"
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
SAMPLE = os.path.join(REPO, "docs", "tmp", "20260328-DSC05024.jpg")
PATCHES = [(200, 60, 60), (60, 160, 90), (90, 110, 200), (210, 170, 120)]

out = sys.argv[1]
icc_dir = os.path.join(out, "icc")
os.makedirs(icc_dir, exist_ok=True)
work = tempfile.mkdtemp()


def run(*args):
    subprocess.run(list(args), check=True)


def magick_out(*args):
    return subprocess.run(["magick", *args], check=True, capture_output=True, text=True).stdout


patch = Image.new("RGB", (32, 8))
for i, c in enumerate(PATCHES):
    patch.paste(c, (8 * i, 0, 8 * i + 8, 8))
patch_png = os.path.join(work, "patch.png")
patch.save(patch_png)

p = lambda name: os.path.join(icc_dir, name)
adobe = f"{ICC}/AdobeRGB1998.icc"
prophoto = f"{ICC}/ProPhotoRGB.icc"

run("magick", patch_png, "-profile", adobe, "-depth", "8", f"PNG24:{p('adobe.png')}")
run("magick", patch_png, "-depth", "16", "-alpha", "set", "-channel", "A", "-evaluate", "set", "32896",
    "+channel", "-profile", adobe, f"PNG64:{p('adobe16.png')}")
run("magick", patch_png, "-profile", adobe, "-quality", "100", "-sampling-factor", "1x1", p("adobe.jpg"))
patch.save(p("adobe.webp"), lossless=True, icc_profile=open(adobe, "rb").read())
run("magick", patch_png, "-profile", prophoto, "-depth", "8", f"PNG24:{p('prophoto.png')}")

lrc_icc = os.path.join(work, "lrc_srgb.icc")
run("magick", SAMPLE, lrc_icc)
desc = magick_out(lrc_icc, "-format", "%[icc:description]", "info:").strip()
assert desc == "sRGB IEC61966-2.1", desc
run("magick", patch_png, "-profile", lrc_icc, "-quality", "100", "-sampling-factor", "1x1", p("srgb_lrc.jpg"))

patch.save(p("broken_icc.png"), icc_profile=b"not an icc profile")

Image.new("RGBA", (12000, 12000), (0, 0, 0, 0)).save(os.path.join(out, "over_limit.png"), optimize=True)

# 確認: 割り当てで画素値が変わっていないこと・16bit とアルファ
for name in ["adobe.png", "adobe16.png", "prophoto.png", "broken_icc.png"]:
    print(name, magick_out(p(name), "-format", "%[channels] %[depth]bit", "info:"))
im = Image.open(p("adobe.webp"))
assert [im.getpixel((8 * i + 4, 4)) for i in range(4)] == PATCHES

# 期待値（ImageMagick の相対的色域維持）
srgb = f"{ICC}/sRGB.icc"
for name in ["adobe.png", "prophoto.png"]:
    conv = os.path.join(work, "conv_" + name)
    run("magick", p(name), "-intent", "relative", "-profile", srgb, "-depth", "8", conv)
    print(name, "->", [Image.open(conv).convert("RGB").getpixel((8 * i + 4, 4)) for i in range(4)])
conv = os.path.join(work, "conv_adobe16.png")
run("magick", p("adobe16.png"), "-intent", "relative", "-profile", srgb, "-clamp", "-background", "white", "-flatten",
    "-depth", "8", conv)
print("adobe16.png (変換してから白に合成) ->", [Image.open(conv).convert("RGB").getpixel((8 * i + 4, 4)) for i in range(4)])
conv = os.path.join(work, "flat_first_adobe16.png")
run("magick", p("adobe16.png"), "-background", "white", "-flatten", "-intent", "relative", "-profile", srgb,
    "-clamp", "-depth", "8", conv)
print("adobe16.png (白に合成してから変換。誤った順序) ->",
      [Image.open(conv).convert("RGB").getpixel((8 * i + 4, 4)) for i in range(4)])
