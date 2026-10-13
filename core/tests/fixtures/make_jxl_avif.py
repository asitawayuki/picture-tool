"""tests/jxl_avif_input.rs のフィクスチャ（*.jxl / *.avif。各ファイルの中身はテスト冒頭の表）を作る。

使い方: uv run --no-project --with pillow --with brotli python make_jxl_avif.py <出力先>
要件: Pillow（AVIF 対応版）、brotli と、libjxl 入りの ffmpeg（7 以降。表示行列を JPEG XL の向きに書く）。
中間ファイル（src_*, plain.jxl）はコミットしない。
"""
import struct, subprocess, sys, os
import brotli
from PIL import Image

out = sys.argv[1]
os.makedirs(out, exist_ok=True)

# 40x20: left half red, right half blue
img = Image.new("RGB", (40, 20), (255, 0, 0))
img.paste((0, 0, 255), (20, 0, 40, 20))

def exif(orientation):
    e = Image.Exif()
    e[0x010F] = "TestMake"
    e[0x0110] = "TestModel"
    e[0x0112] = orientation
    return e

# JPEG source with EXIF orientation 6 (for ffmpeg)
img.save(f"{out}/src_o6.jpg", exif=exif(6), quality=95)
img.save(f"{out}/src.png")

def ff(src, dst, *extra):
    subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", *extra,
                    "-i", src, "-c:v", "libjxl", "-distance", "0", dst], check=True)

# bare codestream, no orientation
ff(f"{out}/src.png", f"{out}/plain.jxl")
# codestream orientation via display matrix (ffmpeg >= 7)
ff(f"{out}/src_o6.jpg", f"{out}/rotated.jxl", "-noautorotate")

def box(t, payload):
    return struct.pack(">I", 8 + len(payload)) + t + payload

# Container: signature, ftyp, Exif (4-byte tiff offset + TIFF), jxlc
cs = open(f"{out}/plain.jxl", "rb").read()
assert cs[:2] == b"\xff\x0a", cs[:12]
tiff = exif(6).tobytes()
if tiff.startswith(b"Exif\x00\x00"):
    tiff = tiff[6:]
assert tiff[:2] in (b"II", b"MM"), tiff[:8]
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"Exif", b"\x00\x00\x00\x00" + tiff)
             + box(b"jxlc", cs))
open(f"{out}/exif_o6.jxl", "wb").write(container)

# 同じ内容で tiff_header_offset=6（TIFF の前に "Exif\0\0" を置く書き手がいる）
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"Exif", b"\x00\x00\x00\x06Exif\x00\x00" + tiff)
             + box(b"jxlc", cs))
open(f"{out}/exif_offset.jxl", "wb").write(container)

# 同じ内容で Exif を Brotli 圧縮ボックス（brob）に入れる。cjxl の既定の書き方
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"brob", b"Exif" + brotli.compress(b"\x00\x00\x00\x00" + tiff))
             + box(b"jxlc", cs))
open(f"{out}/exif_brob.jxl", "wb").write(container)

# AVIF: EXIF Orientation=6 but no irot. Pillow would turn Orientation into irot,
# so write a placeholder tag 0x0113 and patch its id to 0x0112 afterwards.
e = Image.Exif()
e[0x010F] = "TestMake"
e[0x0110] = "TestModel"
e[0x0113] = 6
img.save(f"{out}/exif_o6.avif", exif=e, quality=90)
data = open(f"{out}/exif_o6.avif", "rb").read()
needle = bytes.fromhex("0113 0003 00000001 0006".replace(" ", ""))  # big-endian IFD entry
assert data.count(needle) == 1 and b"irot" not in data
open(f"{out}/exif_o6.avif", "wb").write(data.replace(needle, bytes.fromhex("0112") + needle[2:]))

# 分割コードストリーム（jxlp ×2）＋ Exif。libjxl はメタデータを後ろに置くときなどにこう書く
half = len(cs) // 2
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"Exif", b"\x00\x00\x00\x00" + tiff)
             + box(b"jxlp", struct.pack(">I", 0) + cs[:half])
             + box(b"jxlp", struct.pack(">I", 0x80000001) + cs[half:]))
open(f"{out}/split.jxl", "wb").write(container)

# Brotli として壊れた brob（中身の種類は Exif）。画素・寸法の読み出しはこれに触れてはいけない
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"brob", b"Exif" + b"\xff" * 32)
             + box(b"jxlc", cs))
open(f"{out}/broken_brob.jxl", "wb").write(container)

# 展開すると 8MiB になる Exif の brob（展開爆弾の縮小版）
bomb = brotli.compress(b"\x00\x00\x00\x00" + tiff + b"\x00" * (8 << 20))
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"brob", b"Exif" + bomb)
             + box(b"jxlc", cs))
open(f"{out}/exif_bomb.jxl", "wb").write(container)

# コンテナの jxlc の中に、さらにコンテナ（brob の Exif ＋ jxlc）を入れたもの。仕様上 jxlc の中身は
# コードストリームなので不正。内側を展開させないよう、読み手は拒否しなければならない
inner = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
         + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
         + box(b"brob", b"Exif" + brotli.compress(b"\x00\x00\x00\x00" + tiff))
         + box(b"jxlc", cs))
container = (box(b"JXL ", b"\x0d\x0a\x87\x0a")
             + box(b"ftyp", b"jxl \x00\x00\x00\x00jxl ")
             + box(b"jxlc", inner))
open(f"{out}/nested.jxl", "wb").write(container)

# 14000x13000 の単色の非可逆 JPEG XL。出力（RGB 8bit）が 546MB で、512MB の上限を超える。
# 中身は正当なので、上限で弾かなければデコーダは実際に数 GB を確保しにいく
subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi",
                "-i", "color=c=red:s=14000x13000", "-frames:v", "1", "-pix_fmt", "rgb24",
                "-c:v", "libjxl", "-distance", "1", "-effort", "7", f"{out}/over_limit.jxl"],
               check=True)

# 約 24MP（6000x4000）の非可逆 JPEG XL。非可逆の既定ではデコーダが画素あたり出力の数倍の
# 作業領域を使う（20MP 前後で 512MB を超える）。単色に近い図柄なので小さく収まる
big = Image.new("RGB", (6000, 4000), (255, 0, 0))
big.paste((0, 0, 255), (3000, 0, 6000, 4000))
big.save(f"{out}/src_big.png")
subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", f"{out}/src_big.png",
                "-c:v", "libjxl", "-distance", "1", f"{out}/lossy_24mp.jxl"], check=True)

# AVIF の向き（irot / imir）。Pillow は Exif の向きを irot / imir に変換して書く。
# 64x32 を 4 分割し、左上=赤、右上=緑、左下=青、右下=黄
quad = Image.new("RGB", (64, 32))
for x0, y0, color in [(0, 0, (255, 0, 0)), (32, 0, (0, 255, 0)),
                      (0, 16, (0, 0, 255)), (32, 16, (255, 255, 0))]:
    quad.paste(color, (x0, y0, x0 + 32, y0 + 16))
for o in range(1, 9):
    e = Image.Exif()
    e[0x0112] = o
    quad.save(f"{out}/orient_{o}.avif", exif=e, quality=90)
    assert (b"irot" in open(f"{out}/orient_{o}.avif", "rb").read()) == (o in (3, 5, 6, 7, 8)), o

for f in sorted(os.listdir(out)):
    print(f, os.path.getsize(f"{out}/{f}"))
