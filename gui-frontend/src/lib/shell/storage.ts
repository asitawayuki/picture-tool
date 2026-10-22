/**
 * localStorage の読み書き。使えない環境（WebKitGTK で永続が不安定な例が知られている）
 * でも例外を外へ出さない。読めなければ `null`（＝既定値に倒れる）、書けなければ黙って捨てる。
 * 失って困る情報（カラム幅・テーマ）にしか使わないので、トーストも出さない。
 */
export function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function writeStorage(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 保存できないだけで操作は続けられる
  }
}
