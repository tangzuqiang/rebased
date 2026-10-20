export function joinLines(lines: Array<string | null>) {
  return lines.filter((line): line is string => line !== null).join("\n");
}

export function matchLines(a: string[], b: string[]): Array<[number, number]> {
  const pairs: Array<[number, number]> = [];
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) {
    pairs.push([head, head]);
    head += 1;
  }
  let tailA = a.length;
  let tailB = b.length;
  while (tailA > head && tailB > head && a[tailA - 1] === b[tailB - 1]) {
    tailA -= 1;
    tailB -= 1;
  }
  const n = tailA - head;
  const m = tailB - head;
  if (n > 0 && m > 0 && n * m <= 4_000_000) {
    const width = m + 1;
    const dp = new Uint32Array((n + 1) * width);
    for (let i = n - 1; i >= 0; i--) {
      for (let j = m - 1; j >= 0; j--) {
        dp[i * width + j] =
          a[head + i] === b[head + j]
            ? dp[(i + 1) * width + j + 1] + 1
            : Math.max(dp[(i + 1) * width + j], dp[i * width + j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < n && j < m) {
      if (a[head + i] === b[head + j]) {
        pairs.push([head + i, head + j]);
        i += 1;
        j += 1;
      } else if (dp[(i + 1) * width + j] >= dp[i * width + j + 1]) {
        i += 1;
      } else {
        j += 1;
      }
    }
  }
  for (let t = 0; t < a.length - tailA; t++) pairs.push([tailA + t, tailB + t]);
  return pairs;
}

/**
 * 每个元素对应三方对齐后的一行（可含多行文本，null 表示结果中没有该行）。
 * 把编辑后的整段文本重新分配回这些行：修改的行留在原位，新增的行跟随前一行，删除的行置空。
 */
export function remapLines(lines: Array<string | null>, text: string): Array<string | null> {
  const owners: number[] = [];
  lines.forEach((line, row) => {
    if (line === null) return;
    for (let k = line.split("\n").length; k > 0; k--) owners.push(row);
  });
  const oldLines = owners.length ? joinLines(lines).split("\n") : [];
  const newLines = text.split("\n");
  if (oldLines.length === newLines.length && oldLines.every((line, i) => line === newLines[i])) return lines;
  const fallbackRow = owners[0] ?? (lines.length ? 0 : -1);
  const target: number[] = new Array(newLines.length).fill(-1);
  let pa = 0;
  let pb = 0;
  for (const [k, j] of [...matchLines(oldLines, newLines), [oldLines.length, newLines.length]]) {
    const gapOld = k - pa;
    for (let t = 0; pb + t < j; t++) {
      if (t < gapOld) target[pb + t] = owners[pa + t];
      else if (gapOld > 0) target[pb + t] = owners[k - 1];
      else if (pa > 0) target[pb + t] = owners[pa - 1];
      else target[pb + t] = k < oldLines.length ? owners[k] : fallbackRow;
    }
    if (k < oldLines.length) target[j] = owners[k];
    pa = k + 1;
    pb = j + 1;
  }
  const buckets = new Map<number, string[]>();
  newLines.forEach((line, j) => {
    const row = target[j];
    if (row < 0) return;
    const list = buckets.get(row) ?? [];
    list.push(line);
    buckets.set(row, list);
  });
  return lines.map((_, row) => buckets.get(row)?.join("\n") ?? null);
}
