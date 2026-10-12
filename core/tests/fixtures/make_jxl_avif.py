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

for f in sorted(os.listdir(out)):
    print(f, os.path.getsize(f"{out}/{f}"))
