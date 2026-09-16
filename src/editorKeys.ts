export function commentPrefix(path: string) {
  const name = path.split(/[\\/]/).pop()?.toLowerCase() ?? "";
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : name;
  if (["py", "sh", "bash", "zsh", "yml", "yaml", "toml", "rb", "r", "gitignore", "dockerignore", "env"].includes(ext)) {
    return "#";
  }
  if (["sql", "lua"].includes(ext)) return "--";
  if (["bat", "cmd"].includes(ext)) return "rem ";
  return "//";
}

export type EditorKeyOptions = {
  onSave?: () => void;
  onFind?: () => void;
  commentPrefix?: string;
};

function emitInput(el: HTMLTextAreaElement) {
  el.dispatchEvent(new Event("input", { bubbles: true }));
}

export function writeTextarea(el: HTMLTextAreaElement | null | undefined, text: string) {
  if (!el || el.value === text) return;
  el.value = text;
}

function replace(el: HTMLTextAreaElement, start: number, end: number, insert: string, selStart: number, selEnd = selStart) {
  el.focus();
  el.setSelectionRange(start, end);
  const applied = insert
    ? document.execCommand("insertText", false, insert)
    : document.execCommand("delete");
  if (!applied) {
    el.setRangeText(insert, start, end, "select");
  }
  el.setSelectionRange(selStart, selEnd);
  emitInput(el);
}

function lineStart(text: string, pos: number) {
  return text.lastIndexOf("\n", pos - 1) + 1;
}

function lineEnd(text: string, pos: number) {
  const index = text.indexOf("\n", pos);
  return index < 0 ? text.length : index;
}

function selectedLines(el: HTMLTextAreaElement) {
  const text = el.value;
  const from = Math.min(el.selectionStart, el.selectionEnd);
  const to = Math.max(el.selectionStart, el.selectionEnd);
  const start = lineStart(text, from);
  const end = to > from && text[to - 1] === "\n" ? to - 1 : lineEnd(text, to);
  return { text, start, end, from, to };
}

function indent(el: HTMLTextAreaElement, outdent: boolean) {
  const { text, start, end, from, to } = selectedLines(el);
  const block = text.slice(start, end);
  const lines = block.split("\n");
  let deltaStart = 0;
  let deltaEnd = 0;
  const next = lines.map((line, index) => {
    if (outdent) {
      const cut = line.startsWith("\t") ? 1 : line.startsWith("    ") ? 4 : line.match(/^ +/)?.[0].length ?? 0;
      if (index === 0) deltaStart -= Math.min(cut, from - start);
      deltaEnd -= cut;
      return line.slice(cut);
    }
    if (index === 0) deltaStart += 1;
    deltaEnd += 1;
    return `\t${line}`;
  });
  const insert = next.join("\n");
  replace(el, start, end, insert, Math.max(start, from + deltaStart), Math.max(start, to + deltaEnd));
}

function duplicate(el: HTMLTextAreaElement) {
  const text = el.value;
  const from = el.selectionStart;
  const to = el.selectionEnd;
  if (from !== to) {
    const chunk = text.slice(from, to);
    replace(el, to, to, chunk, to, to + chunk.length);
    return;
  }
  const start = lineStart(text, from);
  const end = lineEnd(text, from);
  const line = text.slice(start, end);
  const nl = end < text.length ? "" : "";
  replace(el, end, end, `\n${line}${nl}`, from + 1 + line.length);
}

function deleteLine(el: HTMLTextAreaElement) {
  const text = el.value;
  const start = lineStart(text, el.selectionStart);
  let end = lineEnd(text, el.selectionStart);
  if (end < text.length) end += 1;
  else if (start > 0) {
    replace(el, start - 1, end, "", start - 1);
    return;
  }
  replace(el, start, end, "", start);
}

function moveLines(el: HTMLTextAreaElement, dir: -1 | 1) {
  const { text, start, end } = selectedLines(el);
  const from = el.selectionStart;
  const to = el.selectionEnd;
  if (dir < 0) {
    if (start === 0) return;
    const prevStart = lineStart(text, start - 1);
    const prev = text.slice(prevStart, start);
    const block = text.slice(start, end);
    const sep = text[end] === "\n" || end === text.length ? "" : "";
    const insert = `${block}\n${prev.endsWith("\n") ? prev.slice(0, -1) : prev}${sep}`;
    replace(el, prevStart, end, insert, from - (start - prevStart), to - (start - prevStart));
    return;
  }
  if (end >= text.length) return;
  const nextEnd = lineEnd(text, end + 1);
  const next = text.slice(end + 1, nextEnd);
  const block = text.slice(start, end);
  replace(el, start, nextEnd, `${next}\n${block}`, from + next.length + 1, to + next.length + 1);
}

function toggleComment(el: HTMLTextAreaElement, prefix: string) {
  const { text, start, end, from, to } = selectedLines(el);
  const lines = text.slice(start, end).split("\n");
  const token = prefix.trimEnd();
  const allCommented = lines.every((line) => !line.trim() || line.trimStart().startsWith(token));
  let delta = 0;
  const next = lines.map((line, index) => {
    if (!line.trim()) return line;
    const indent = line.match(/^\s*/)?.[0] ?? "";
    if (allCommented) {
      const cut = line.slice(indent.length).startsWith(`${token} `)
        ? token.length + 1
        : line.slice(indent.length).startsWith(token)
          ? token.length
          : 0;
      if (index === 0) delta -= Math.min(cut, Math.max(0, from - start - indent.length));
      return indent + line.slice(indent.length + cut);
    }
    if (index === 0) delta += token.length + 1;
    return `${indent}${token} ${line.slice(indent.length)}`;
  });
  const insert = next.join("\n");
  const shift = allCommented ? delta : lines.length === 1 ? token.length + 1 : insert.length - (end - start);
  replace(el, start, end, insert, from + (allCommented ? delta : token.length + 1), to + shift);
}

function toggleCase(el: HTMLTextAreaElement) {
  const from = el.selectionStart;
  const to = el.selectionEnd;
  if (from === to) return;
  const chunk = el.value.slice(from, to);
  const upper = chunk.toUpperCase();
  const next = chunk === upper ? chunk.toLowerCase() : upper;
  replace(el, from, to, next, from, from + next.length);
}

function joinLines(el: HTMLTextAreaElement) {
  const text = el.value;
  const start = lineStart(text, el.selectionStart);
  const end = lineEnd(text, el.selectionStart);
  if (end >= text.length) return;
  const left = text.slice(start, end).trimEnd();
  const right = text.slice(end + 1, lineEnd(text, end + 1)).trimStart();
  replace(el, start, lineEnd(text, end + 1), `${left} ${right}`.trimEnd(), start + left.length + (right ? 1 : 0));
}

function startNewLine(el: HTMLTextAreaElement) {
  const end = lineEnd(el.value, el.selectionStart);
  replace(el, end, end, "\n", end + 1);
}

function goToLine(el: HTMLTextAreaElement) {
  const lines = el.value.split("\n");
  const raw = window.prompt("转到行", "1");
  const line = Number(raw);
  if (!Number.isFinite(line) || line < 1) return;
  const index = Math.min(lines.length, Math.floor(line)) - 1;
  let pos = 0;
  for (let i = 0; i < index; i += 1) pos += lines[i].length + 1;
  el.focus();
  el.setSelectionRange(pos, pos);
  const lineHeight = Number.parseFloat(getComputedStyle(el).lineHeight) || 18;
  el.scrollTop = Math.max(0, index * lineHeight - el.clientHeight / 2);
}

export function findInTextarea(el: HTMLTextAreaElement, query: string, reverse = false) {
  if (!query) return false;
  const text = el.value;
  const needle = query.toLowerCase();
  const hay = text.toLowerCase();
  const from = reverse ? el.selectionStart : el.selectionEnd;
  let index = reverse ? hay.lastIndexOf(needle, Math.max(0, from - 1)) : hay.indexOf(needle, from);
  if (index < 0) {
    index = reverse ? hay.lastIndexOf(needle) : hay.indexOf(needle);
  }
  if (index < 0) return false;
  el.focus();
  el.setSelectionRange(index, index + query.length);
  const lineHeight = Number.parseFloat(getComputedStyle(el).lineHeight) || 18;
  const line = text.slice(0, index).split("\n").length - 1;
  el.scrollTop = Math.max(0, line * lineHeight - 40);
  return true;
}

export function handleIdeEditorKey(event: KeyboardEvent, options: EditorKeyOptions = {}) {
  const el = event.target;
  if (!(el instanceof HTMLTextAreaElement) || el.disabled) return false;
  const ctrl = event.ctrlKey || event.metaKey;
  const shift = event.shiftKey;
  const alt = event.altKey;
  const key = event.key;
  const locked = el.readOnly;

  if (ctrl && !alt && key.toLowerCase() === "s") {
    event.preventDefault();
    if (!locked) options.onSave?.();
    return true;
  }
  if (ctrl && !alt && key.toLowerCase() === "f") {
    event.preventDefault();
    options.onFind?.();
    return true;
  }
  if (ctrl && !alt && !shift && key.toLowerCase() === "g") {
    event.preventDefault();
    goToLine(el);
    return true;
  }
  if (locked) return false;
  if (ctrl && !alt && key.toLowerCase() === "z") {
    event.preventDefault();
    document.execCommand(shift ? "redo" : "undo");
    emitInput(el);
    return true;
  }
  if (key === "Tab" && !ctrl && !alt) {
    event.preventDefault();
    indent(el, shift);
    return true;
  }
  if (ctrl && !alt && !shift && key.toLowerCase() === "d") {
    event.preventDefault();
    duplicate(el);
    return true;
  }
  if (ctrl && !alt && !shift && key.toLowerCase() === "y") {
    event.preventDefault();
    deleteLine(el);
    return true;
  }
  if (ctrl && !alt && !shift && key === "/") {
    event.preventDefault();
    toggleComment(el, options.commentPrefix || "//");
    return true;
  }
  if (ctrl && shift && !alt && (key === "ArrowUp" || key === "ArrowDown")) {
    event.preventDefault();
    moveLines(el, key === "ArrowUp" ? -1 : 1);
    return true;
  }
  if (ctrl && shift && !alt && key.toLowerCase() === "u") {
    event.preventDefault();
    toggleCase(el);
    return true;
  }
  if (ctrl && shift && !alt && key.toLowerCase() === "j") {
    event.preventDefault();
    joinLines(el);
    return true;
  }
  if (!ctrl && !alt && shift && key === "Enter") {
    event.preventDefault();
    startNewLine(el);
    return true;
  }
  return false;
}
