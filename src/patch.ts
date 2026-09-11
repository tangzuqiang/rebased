import type { DiffLine } from "./types";

export function parsePatch(patch: string): DiffLine[] {
  const rows: DiffLine[] = [];
  let oldNo = 0;
  let newNo = 0;

  for (const raw of patch.replace(/\r\n/g, "\n").split("\n")) {
    if (
      raw.startsWith("diff ") ||
      raw.startsWith("index ") ||
      raw.startsWith("new file") ||
      raw.startsWith("deleted file") ||
      raw.startsWith("--- ") ||
      raw.startsWith("+++ ")
    ) {
      continue;
    }
    const hunk = raw.match(/^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/);
    if (hunk) {
      oldNo = Number(hunk[1]);
      newNo = Number(hunk[2]);
      rows.push({ kind: "hunk", text: raw, oldNo: "", newNo: "" });
      continue;
    }
    if (raw.startsWith("+")) {
      rows.push({ kind: "add", text: raw.slice(1), oldNo: "", newNo: String(newNo) });
      newNo += 1;
      continue;
    }
    if (raw.startsWith("-")) {
      rows.push({ kind: "del", text: raw.slice(1), oldNo: String(oldNo), newNo: "" });
      oldNo += 1;
      continue;
    }
    if (raw.startsWith("\\")) {
      continue;
    }
    rows.push({
      kind: "ctx",
      text: raw.startsWith(" ") ? raw.slice(1) : raw,
      oldNo: String(oldNo),
      newNo: String(newNo),
    });
    if (raw.length) {
      oldNo += 1;
      newNo += 1;
    }
  }
  return rows;
}
