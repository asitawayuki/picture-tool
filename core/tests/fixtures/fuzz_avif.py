#!/usr/bin/env python3
"""AVIF のファズ: 元ファイルのバイトをランダムに書き換え、probe が abort しないことを確かめる。

使い方:
    uv run --no-project python core/tests/fixtures/fuzz_avif.py \
        <probe> <出力dir> [--seed N] [--count N] <元の AVIF...>

元ファイルごとに --count 件（既定 130）、1〜4 バイトを書き換えたファイルを作り、
1 件ずつ probe を子プロセスで開く。数えるのは次の 3 つで、どれも 0 件でなければならない:
  - abort / 異常終了（終了コードが 0 でも 1 でもない）
  - シグナルによる終了
  - 10 秒のタイムアウト
該当したファイルは <出力dir> に残す（正常だったものは消す）。
乱数の種は引数で表示する。同じ種・同じ元ファイルなら同じ書き換えになる。
"""
import os
import random
import subprocess
import sys
import zlib

TIMEOUT_S = 10


def mutate(data: bytes, rng: random.Random) -> bytes:
    out = bytearray(data)
    for _ in range(rng.randint(1, 4)):
        out[rng.randrange(len(out))] = rng.randrange(256)
    return bytes(out)


def classify(probe: str, path: str) -> str:
    try:
        r = subprocess.run([probe, path], capture_output=True, timeout=TIMEOUT_S)
    except subprocess.TimeoutExpired:
        return "timeout"
    if r.returncode < 0:
        return "signal"
    if r.returncode == 0:
        return "ok"
    if r.returncode == 1:
        return "err"
    return "abort"


def main() -> int:
    args = sys.argv[1:]
    seed, count = 20261019, 130
    for flag in ("--seed", "--count"):
        if flag in args:
            i = args.index(flag)
            value = int(args[i + 1])
            del args[i : i + 2]
            if flag == "--seed":
                seed = value
            else:
                count = value
    if len(args) < 3:
        print(__doc__, file=sys.stderr)
        return 2
    probe, out_dir, sources = args[0], args[1], args[2:]
    os.makedirs(out_dir, exist_ok=True)
    print(f"seed={seed} count/file={count} sources={len(sources)}")

    total = {"ok": 0, "err": 0, "abort": 0, "signal": 0, "timeout": 0}
    for src in sources:
        name = os.path.splitext(os.path.basename(src))[0]
        data = open(src, "rb").read()
        # 元ファイルごとに独立した乱数列にする（引数の順序や個数で結果が変わらない）
        rng = random.Random(seed ^ zlib.crc32(name.encode()))
        counts = {k: 0 for k in total}
        for n in range(count):
            path = os.path.join(out_dir, f"{name}_{n:04d}.avif")
            with open(path, "wb") as f:
                f.write(mutate(data, rng))
            kind = classify(probe, path)
            counts[kind] += 1
            if kind in ("ok", "err"):
                os.remove(path)
            else:
                print(f"  {kind}: {path}")
        for k in total:
            total[k] += counts[k]
        print(f"{name}: " + " ".join(f"{k}={v}" for k, v in counts.items()))
    print("TOTAL " + " ".join(f"{k}={v}" for k, v in total.items()) + f" n={sum(total.values())}")
    bad = total["abort"] + total["signal"] + total["timeout"]
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
