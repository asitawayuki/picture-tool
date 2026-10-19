"""AVIF デコーダの検証用フィクスチャ（avif/*.avif）を作る。各ファイルの中身の表は tests/jxl_avif_input.rs 冒頭。

使い方: uv run --no-project --with pillow==12.3.0 python make_avif_decoder.py <出力先>
使ったツールと版: ffmpeg 8.1.3（libaom-av1。AV1 のエンコードと、2 フレームの確認のデコードに libdav1d）、
aomenc（libaom 3.13.3。ffmpeg と同じ libaom。alpha_huge.avif のアルファだけ。ffmpeg は約 2.7 億画素を
超えるフレームを扱えない）、Pillow 12.3.0（base_64x32.avif だけ。Pillow の版で中のバイト列が変わる）。

- 主画像とアルファは ffmpeg で別々に `-f obu` でエンコードし、このスクリプトが AVIF に組む
  （ftyp / meta（hdlr・pitm・iloc・iinf・iref・iprp）/ mdat）。Temporal Delimiter は除く。
- 行列・範囲・深度・間引きはシーケンスヘッダに書かれた値で決まる。ffmpeg が黙って別の値を
  書くことがあるので、書かれた値をこのスクリプトが解析して assert する。シーケンスヘッダの数も見る。
- crash.avif は元ファイル名と、壊したオフセット・元の値を表示する。
- 中間ファイルは一時ディレクトリに置き、出力先に残さない。
"""
import os, struct, subprocess, sys, tempfile
from PIL import Image

out = sys.argv[1]
os.makedirs(out, exist_ok=True)
tmp = tempfile.TemporaryDirectory()

W, H = 33, 17  # 奇数寸法（間引いたクロマの端数を通す）

# --- ISOBMFF ---

def box(t, payload):
    return struct.pack(">I", 8 + len(payload)) + t + payload

def fullbox(t, version, flags, payload):
    return box(t, struct.pack(">I", (version << 24) | flags) + payload)

# --- AV1 OBU ---

OBU_SEQUENCE_HEADER, OBU_TEMPORAL_DELIMITER, OBU_FRAME_HEADER, OBU_FRAME = 1, 2, 3, 6

def leb128(data, at):
    value = 0
    for i in range(8):
        b = data[at + i]
        value |= (b & 0x7F) << (i * 7)
        if not b & 0x80:
            return value, at + i + 1
    raise ValueError("leb128 too long")

def obus(data):
    """(型, OBU 全体の開始, ペイロードの開始, ペイロードの終わり) の列。全体を過不足なく読む"""
    at, result = 0, []
    while at < len(data):
        header = data[at]
        assert header & 0b10, "OBU without size field"
        size, payload = leb128(data, at + 1 + ((header >> 2) & 1))
        result.append(((header >> 3) & 0xF, at, payload, payload + size))
        at = payload + size
    assert at == len(data)
    return result

def strip_temporal_delimiters(data):
    return b"".join(data[s:e] for t, s, _, e in obus(data) if t != OBU_TEMPORAL_DELIMITER)

class Bits:
    def __init__(self, data):
        self.data, self.pos = data, 0

    def f(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.data[self.pos >> 3] >> (7 - (self.pos & 7))) & 1)
            self.pos += 1
        return v

    def uvlc(self):
        zeros = 0
        while not self.f(1):
            zeros += 1
        return self.f(zeros) + (1 << zeros) - 1

def parse_sequence_header(p):
    """AV1 仕様 5.5 のシーケンスヘッダから、色と寸法に関わる値を読む"""
    b = Bits(p)
    sh = {"profile": b.f(3)}
    b.f(1)  # still_picture
    reduced = b.f(1)
    if reduced:
        sh["level"], sh["tier"] = b.f(5), 0
    else:
        decoder_model, buffer_delay_len = False, 0
        if b.f(1):  # timing_info_present_flag
            b.f(32); b.f(32)
            if b.f(1):
                b.uvlc()
            decoder_model = bool(b.f(1))
            if decoder_model:
                buffer_delay_len = b.f(5) + 1
                b.f(32); b.f(5); b.f(5)
        initial_display_delay = b.f(1)
        for i in range(b.f(5) + 1):
            b.f(12)
            level = b.f(5)
            tier = b.f(1) if level > 7 else 0
            if i == 0:
                sh["level"], sh["tier"] = level, tier
            if decoder_model and b.f(1):
                b.f(buffer_delay_len); b.f(buffer_delay_len); b.f(1)
            if initial_display_delay and b.f(1):
                b.f(4)
    wbits, hbits = b.f(4) + 1, b.f(4) + 1
    sh["max_width"], sh["max_height"] = b.f(wbits) + 1, b.f(hbits) + 1
    if not reduced and b.f(1):  # frame_id_numbers_present_flag
        b.f(4); b.f(3)
    b.f(3)  # use_128x128_superblock, enable_filter_intra, enable_intra_edge_filter
    if not reduced:
        b.f(4)  # interintra_compound, masked_compound, warped_motion, dual_filter
        order_hint = b.f(1)
        if order_hint:
            b.f(2)
        force_sct = 2 if b.f(1) else b.f(1)
        if force_sct > 0 and not b.f(1):
            b.f(1)
        if order_hint:
            b.f(3)
    b.f(3)  # superres, cdef, restoration
    high = b.f(1)
    if sh["profile"] == 2 and high:
        sh["depth"] = 12 if b.f(1) else 10
    else:
        sh["depth"] = 10 if high else 8
    mono = 0 if sh["profile"] == 1 else b.f(1)
    cp = tc = mc = 2
    if b.f(1):
        cp, tc, mc = b.f(8), b.f(8), b.f(8)
    sh["matrix"] = mc
    if mono:
        sh["full_range"], ssx, ssy = b.f(1), 1, 1
    elif cp == 1 and tc == 13 and mc == 0:
        sh["full_range"], ssx, ssy = 1, 0, 0
    else:
        sh["full_range"] = b.f(1)
        if sh["profile"] == 0:
            ssx, ssy = 1, 1
        elif sh["profile"] == 1:
            ssx, ssy = 0, 0
        elif sh["depth"] == 12:
            ssx = b.f(1)
            ssy = b.f(1) if ssx else 0
        else:
            ssx, ssy = 1, 0
    sh["mono"], sh["ssx"], sh["ssy"] = mono, ssx, ssy
    sh["sampling"] = "400" if mono else {(1, 1): "420", (1, 0): "422", (0, 0): "444"}[(ssx, ssy)]
    return sh

def sequence_headers(av1):
    return [parse_sequence_header(av1[p:e]) for t, _, p, e in obus(av1) if t == OBU_SEQUENCE_HEADER]

def check(av1, depth, sampling, rng, matrix, count=1):
    """書かれたシーケンスヘッダが意図どおりか確かめ、最初のヘッダを返す"""
    shs = sequence_headers(av1)
    assert len(shs) == count, f"sequence headers: {len(shs)} != {count}"
    sh = shs[0]
    got = (sh["depth"], sh["sampling"], "full" if sh["full_range"] else "lim", sh["matrix"])
    assert got == (depth, sampling, rng, matrix), f"{got} != {(depth, sampling, rng, matrix)}"
    return sh

# --- AVIF ---

def av1c(sh):
    """av1C（AV1-ISOBMFF 2.3.3）の 4 バイト。configOBUs は付けない"""
    return bytes([
        0x81,
        (sh["profile"] << 5) | sh["level"],
        (sh["tier"] << 7) | ((sh["depth"] > 8) << 6) | ((sh["depth"] == 12) << 5)
        | (sh["mono"] << 4) | (sh["ssx"] << 3) | (sh["ssy"] << 2),
        0,
    ])

def item_properties(sh):
    channels = 1 if sh["mono"] else 3
    return [
        fullbox(b"ispe", 0, 0, struct.pack(">II", sh["max_width"], sh["max_height"])),
        fullbox(b"pixi", 0, 0, bytes([channels] + [sh["depth"]] * channels)),
        box(b"av1C", av1c(sh)),
    ]

def make_avif(color, alpha=None, premultiplied=False):
    """主画像（item 1）とアルファ（item 2）の AV1 データから AVIF を組む"""
    items = [color] + ([alpha] if alpha is not None else [])
    props = item_properties(sequence_headers(color)[0])
    assoc = [(1, [1, 2, 0x80 | 3])]  # av1C は essential
    refs = b""
    if alpha is not None:
        props += item_properties(sequence_headers(alpha)[0])
        props.append(fullbox(b"auxC", 0, 0, b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha\0"))
        assoc.append((2, [4, 5, 0x80 | 6, 7]))
        refs = box(b"auxl", struct.pack(">HHH", 2, 1, 1))
        if premultiplied:
            refs += box(b"prem", struct.pack(">HHH", 1, 1, 2))
        refs = fullbox(b"iref", 0, 0, refs)
    ipma = struct.pack(">I", len(assoc)) + b"".join(
        struct.pack(">HB", item, len(ix)) + bytes(ix) for item, ix in assoc)
    iinf = fullbox(b"iinf", 0, 0, struct.pack(">H", len(items)) + b"".join(
        fullbox(b"infe", 2, 0, struct.pack(">HH", i + 1, 0) + b"av01\0") for i in range(len(items))))
    ftyp = box(b"ftyp", b"avif" + b"\0\0\0\0" + b"avifmif1miaf")

    def meta(mdat_start):
        iloc, at = b"", mdat_start
        for i, data in enumerate(items):
            iloc += struct.pack(">HHHII", i + 1, 0, 1, at, len(data))
            at += len(data)
        return fullbox(b"meta", 0, 0, b"".join([
            fullbox(b"hdlr", 0, 0, b"\0\0\0\0pict" + b"\0" * 12 + b"\0"),
            fullbox(b"pitm", 0, 0, struct.pack(">H", 1)),
            fullbox(b"iloc", 0, 0, bytes([0x44, 0x00]) + struct.pack(">H", len(items)) + iloc),
            iinf, refs,
            box(b"iprp", box(b"ipco", b"".join(props)) + box(b"ipma", b"\0\0\0\0" + ipma)),
        ]))

    # meta の長さはオフセットの値によらないので、仮に組んで長さを測ってから組み直す
    mdat_start = len(ftyp) + len(meta(0)) + 8
    return ftyp + meta(mdat_start) + box(b"mdat", b"".join(items))

def write(name, data):
    with open(f"{out}/{name}", "wb") as f:
        f.write(data)

# --- エンコード ---

PIX_FMT = {
    (8, "420"): "yuv420p", (8, "422"): "yuv422p", (8, "444"): "yuv444p", (8, "400"): "gray",
    (10, "420"): "yuv420p10le", (10, "444"): "yuv444p10le", (10, "400"): "gray10le",
    (12, "422"): "yuv422p12le",
}
# YCgCo は ffmpeg の色変換（swscale）が対応しておらず、-colorspace で指定すると変換の組み立てに
# 失敗する。libaom 自身の設定で行列の値だけを書かせる（画素は YCbCr のまま。照合には影響しない）
COLORSPACE = {0: ["-colorspace", "rgb"], 1: ["-colorspace", "bt709"], 2: ["-colorspace", "unknown"],
              4: ["-colorspace", "fcc"], 6: ["-colorspace", "smpte170m"],
              8: ["-aom-params", "matrix-coefficients=ycgco"]}
RANGE = {"lim": "tv", "full": "pc"}

def encode(input_args, depth, sampling, rng, matrix, frames=1, stdin=None):
    """ffmpeg（libaom-av1）で AV1 にエンコードし、Temporal Delimiter を除いたデータを返す"""
    pix_fmt = "gbrp" if matrix == 0 and depth == 8 else \
        "gbrp10le" if matrix == 0 else PIX_FMT[(depth, sampling)]
    obu = f"{tmp.name}/enc.obu"
    still = ["-still-picture", "1"] if frames == 1 else ["-lag-in-frames", "0"]
    subprocess.run(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", *input_args,
                    "-frames:v", str(frames), "-pix_fmt", pix_fmt,
                    *COLORSPACE[matrix], "-color_range", RANGE[rng],
                    "-c:v", "libaom-av1", "-crf", "20", "-cpu-used", "6", *still,
                    "-f", "obu", obu], check=True, input=stdin)
    with open(obu, "rb") as f:
        return strip_temporal_delimiters(f.read())

def color(depth, sampling, rng, matrix, frames=1):
    """33x17 の testsrc2（非単色。2 フレーム目は 1 フレーム目と図柄が違う）

    testsrc2 は寸法を偶数に丸めるので、1 画素大きく作って RGB のまま切り抜く。
    RGB から gray への変換は ffmpeg が全レンジに固定する（-color_range tv が効かない）ので、
    モノクロは指定の範囲の YUV に変換してから Y 面を取り出す"""
    src = f"testsrc2=s={W + 1}x{H + 1}:r=1,format=rgb24,crop={W}:{H}:0:0"
    if sampling == "400":
        yuv = "yuv444p" if depth == 8 else f"yuv444p{depth}le"
        src += f",scale=out_range={RANGE[rng]},format={yuv},extractplanes=y"
    av1 = encode(["-f", "lavfi", "-i", src], depth, sampling, rng, matrix, frames)
    sh = check(av1, depth, sampling, rng, matrix)
    assert (sh["max_width"], sh["max_height"]) == (W, H)
    return av1

def alpha(depth, sampling, rng):
    """横方向のグラデーション（0〜最大値を含む）。生の値をそのまま渡し、範囲の変換をさせない"""
    top = (1 << depth) - 1
    row = [round(x * top / (W - 1)) for x in range(W)]
    if depth == 8:
        raw, src_fmt = bytes(row) * H, "gray"
    else:
        raw, src_fmt = struct.pack(f"<{W}H", *row) * H, f"gray{depth}le"
    av1 = encode(["-f", "rawvideo", "-pix_fmt", src_fmt, "-s", f"{W}x{H}", "-color_range", RANGE[rng],
                  "-i", "-"], depth, sampling, rng, 2, stdin=raw)
    check(av1, depth, sampling, rng, 2)
    return av1

# --- 画素一致の 20 枚 ---

PLAIN = {
    "p8_420_lim_unspec": (8, "420", "lim", 2),
    "p8_422_full_709": (8, "422", "full", 1),
    "p8_444_lim_601": (8, "444", "lim", 6),
    "p8_444_full_fcc": (8, "444", "full", 4),
    "p8_444_lim_ycgco": (8, "444", "lim", 8),
    "p8_444_full_identity": (8, "444", "full", 0),
    "p10_420_lim_709": (10, "420", "lim", 1),
    "p10_444_full_unspec": (10, "444", "full", 2),
    "p10_444_full_identity": (10, "444", "full", 0),
    "p12_422_lim_601": (12, "422", "lim", 6),
    "gray8_full": (8, "400", "full", 2),
    "gray10_lim": (10, "400", "lim", 2),
}
primary = {}
for name, spec in PLAIN.items():
    primary[name] = color(*spec)
    write(f"{name}.avif", make_avif(primary[name]))

WITH_ALPHA = {  # 名前: (主画像, アルファ, prem)
    "p8_420_lim_709_a8": ((8, "420", "lim", 1), (8, "400", "full"), False),
    "p10_420_lim_709_a10": ((10, "420", "lim", 1), (10, "400", "full"), False),
    "p10_420_lim_709_a8": ((10, "420", "lim", 1), (8, "400", "full"), False),
    "p8_420_lim_709_a10": ((8, "420", "lim", 1), (10, "400", "lim"), False),
    "p8_420_lim_709_a420": ((8, "420", "lim", 1), (8, "420", "full"), False),
    "a8_on_gray8": ((8, "400", "full", 2), (8, "400", "full"), False),
    "a8_premul": ((8, "420", "lim", 1), (8, "400", "full"), True),
}
for name, (main, a, prem) in WITH_ALPHA.items():
    write(f"{name}.avif", make_avif(color(*main), alpha(*a), prem))

# 2 フレーム（シーケンスヘッダ 1 個）を 1 アイテムに入れる。avif-decode は最後のフレームを返す
two = color(8, "420", "lim", 1, frames=2)
assert sum(t in (OBU_FRAME, OBU_FRAME_HEADER) for t, *_ in obus(two)) == 2
with open(f"{tmp.name}/two.obu", "wb") as f:
    f.write(b"\x12\x00" + two)  # ffmpeg の obu 読み込みは Temporal Delimiter で区切るので 1 つ付ける
decoded = subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-c:v", "libdav1d",
                          "-f", "obu", "-i", f"{tmp.name}/two.obu", "-f", "rawvideo", "-"],
                         check=True, capture_output=True).stdout
frame_bytes = W * H + 2 * ((W + 1) // 2) * ((H + 1) // 2)
assert len(decoded) == 2 * frame_bytes and decoded[:frame_bytes] != decoded[frame_bytes:]
write("two_frames.avif", make_avif(two))

# --- 回帰とガード ---

# シーケンスヘッダ OBU の直後に、同じバイト列をそのまま複製する
src = primary["p8_420_lim_unspec"]
(_, s, _, e), = [o for o in obus(src) if o[0] == OBU_SEQUENCE_HEADER]
dup = src[:e] + src[s:e] + src[e:]
check(dup, 8, "420", "lim", 2, count=2)
write("two_seqhdr.avif", make_avif(dup))

# 14000x13000 の単色 8bit 4:2:0。RGB 8bit の出力が 546MB で、512MB の上限を超える。
# 中身は正当なので、上限で弾かなければデコーダは実際に確保しにいく
big = encode(["-f", "lavfi", "-i", "color=c=red:s=14000x13000"], 8, "420", "lim", 2)
sh = check(big, 8, "420", "lim", 2)
assert (sh["max_width"], sh["max_height"]) == (14000, 13000)
write("over_limit.avif", make_avif(big))

# 主画像は 64x64 の単色 8bit、アルファは 23200x23200 の単色 8bit モノクロ。アルファの出力（8bit）が
# 538,240,000 バイトで、512MB の上限を超える。宣言を書き換えた代用品ではなく実際にエンコードする
# （代用品はガードを外しても別の理由で落ちるので、ガードの検査にならない）。
# ffmpeg は (w+128)*(h+128) >= INT_MAX/8 のフレームを拒否するので、アルファは aomenc で作る
HUGE = 23200
small = encode(["-f", "lavfi", "-i", "color=c=red:s=64x64"], 8, "420", "lim", 2)
check(small, 8, "420", "lim", 2)
y4m = f"{tmp.name}/huge.y4m"
with open(y4m, "wb") as f:
    f.write(f"YUV4MPEG2 W{HUGE} H{HUGE} F1:1 Ip A1:1 Cmono XCOLORRANGE=FULL\nFRAME\n".encode())
    row = b"\xff" * HUGE
    for _ in range(HUGE):
        f.write(row)
subprocess.run(["aomenc", "--allintra", "--monochrome", "--cpu-used=9", "--end-usage=q",
                "--cq-level=30", "--threads=16", "--limit=1", "--obu", "-q",
                "-o", f"{tmp.name}/huge.obu", y4m], check=True)
os.remove(y4m)
with open(f"{tmp.name}/huge.obu", "rb") as f:
    huge = strip_temporal_delimiters(f.read())
sh = check(huge, 8, "400", "full", 2)
assert (sh["max_width"], sh["max_height"]) == (HUGE, HUGE)
write("alpha_huge.avif", make_avif(small, huge))

# make_jxl_avif.py の quad（64x32、4 色）を Exif なしで保存したもの。crash*.avif の元
quad = Image.new("RGB", (64, 32))
for x0, y0, c in [(0, 0, (255, 0, 0)), (32, 0, (0, 255, 0)),
                  (0, 16, (0, 0, 255)), (32, 16, (255, 255, 0))]:
    quad.paste(c, (x0, y0, x0 + 32, y0 + 16))
quad.save(f"{out}/base_64x32.avif", quality=90)
with open(f"{out}/base_64x32.avif", "rb") as f:
    base = f.read()
assert b"Exif" not in base

# mdat の中身（1 アイテムだけ）を OBU として読み、ファイル内の絶対オフセットを求める
at = 0
while base[at + 4:at + 8] != b"mdat":
    at += struct.unpack(">I", base[at:at + 4])[0]
mdat_start = at + 8
mdat = base[mdat_start:at + struct.unpack(">I", base[at:at + 4])[0]]
spans = {t: (mdat_start + p, mdat_start + e) for t, _, p, e in obus(mdat)}
frame_start, frame_end = spans[OBU_FRAME]

# frame OBU ペイロードの中央（タイルデータ）の 2 バイトを 0xFF にする。差し替え前の core
# （avif-decode / rav1d）はこれで abort する。次の位置は abort しなかったので使わない（実施メモ / Task 2）:
# frame OBU ペイロード先頭（Ok で開ける。spike の 299・300 もこのベースでは Ok）、
# シーケンスヘッダ OBU ペイロード先頭（seq_profile で Err）、frame OBU ペイロードの 3・4 バイト目（Ok）
at = (frame_start + frame_end) // 2
data = bytearray(base)
print(f"crash.avif: base_64x32.avif のタイルデータ（frame OBU ペイロード {frame_start}..{frame_end} の中央）"
      f" offset {at},{at + 1} 0x{data[at]:02X},0x{data[at + 1]:02X} -> 0xFF,0xFF")
data[at:at + 2] = b"\xff\xff"
write("crash.avif", bytes(data))

for f in sorted(os.listdir(out)):
    print(f, os.path.getsize(f"{out}/{f}"))
