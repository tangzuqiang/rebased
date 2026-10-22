import { defaultKeymap, history, historyKeymap, indentWithTab, invertedEffects } from "@codemirror/commands";
import { LanguageDescription, bracketMatching, indentOnInput, syntaxHighlighting } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import {
  Compartment,
  EditorState,
  RangeSetBuilder,
  StateEffect,
  StateField,
  type Extension,
  type Transaction,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  GutterMarker,
  drawSelection,
  gutter,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  type DecorationSet,
  type KeyBinding,
} from "@codemirror/view";
import { classHighlighter } from "@lezer/highlight";
import type { BlockKind, CharRange, LineRange, SideState } from "./mergeModel";

export interface PaneMark {
  range: LineRange;
  kind: BlockKind;
  words: CharRange[];
}

export const setMarks = StateEffect.define<PaneMark[]>();

export function buildDecorations(doc: EditorState["doc"], marks: PaneMark[], words: boolean): DecorationSet {
  const items: Array<{ from: number; to: number; deco: Decoration }> = [];
  for (const mark of marks) {
    const { start, end } = mark.range;
    if (start === end) {
      const below = start >= doc.lines;
      const line = doc.line(Math.min(Math.max(1, start + 1), doc.lines));
      items.push({
        from: line.from,
        to: line.from,
        deco: Decoration.line({ class: `mrg-gap mrg-${mark.kind} ${below ? "mrg-gap-below" : "mrg-gap-above"}` }),
      });
      continue;
    }
    for (let n = start; n < end && n < doc.lines; n += 1) {
      const line = doc.line(n + 1);
      const edge = `${n === start ? " mrg-first" : ""}${n === end - 1 ? " mrg-last" : ""}`;
      items.push({ from: line.from, to: line.from, deco: Decoration.line({ class: `mrg-line mrg-${mark.kind}${edge}` }) });
    }
    if (!words || !mark.words.length) continue;
    const origin = doc.line(Math.min(start + 1, doc.lines)).from;
    for (const word of mark.words) {
      const from = Math.min(doc.length, origin + word.from);
      const to = Math.min(doc.length, origin + word.to);
      if (to > from) items.push({ from, to, deco: Decoration.mark({ class: "mrg-word" }) });
    }
  }
  items.sort((a, b) => a.from - b.from || (a.deco.startSide ?? 0) - (b.deco.startSide ?? 0));
  const builder = new RangeSetBuilder<Decoration>();
  for (const item of items) builder.add(item.from, item.to, item.deco);
  return builder.finish();
}

const wordsEnabled = StateEffect.define<boolean>();

const marksField = StateField.define<{ marks: PaneMark[]; words: boolean; set: DecorationSet }>({
  create: () => ({ marks: [], words: true, set: Decoration.none }),
  update(value, tr) {
    let { marks, words } = value;
    let changed = false;
    for (const effect of tr.effects) {
      if (effect.is(setMarks)) {
        marks = effect.value;
        changed = true;
      } else if (effect.is(wordsEnabled)) {
        words = effect.value;
        changed = true;
      }
    }
    if (changed) return { marks, words, set: buildDecorations(tr.state.doc, marks, words) };
    if (tr.docChanged) return { marks, words, set: value.set.map(tr.changes) };
    return value;
  },
  provide: (field) => EditorView.decorations.from(field, (value) => value.set),
});

export function updateMarks(view: EditorView, marks: PaneMark[], words: boolean) {
  view.dispatch({ effects: [wordsEnabled.of(words), setMarks.of(marks)] });
}

class BlameMarker extends GutterMarker {
  constructor(readonly label: string, readonly title: string) {
    super();
  }
  eq(other: BlameMarker) {
    return other.label === this.label && other.title === this.title;
  }
  toDOM() {
    const el = document.createElement("span");
    el.textContent = this.label;
    el.title = this.title;
    return el;
  }
}

export interface BlameLabel {
  label: string;
  title: string;
}

export function blameGutter(labels: BlameLabel[]): Extension {
  return gutter({
    class: "mrg-blame",
    lineMarker(view, line) {
      const index = view.state.doc.lineAt(line.from).number - 1;
      const item = labels[index];
      if (!item?.label) return null;
      if (index > 0 && labels[index - 1]?.label === item.label) return null;
      return new BlameMarker(item.label, item.title);
    },
    initialSpacer: () => new BlameMarker("0000/00/00 xxxxxxxx", ""),
  });
}

const phrases = EditorState.phrases.of({
  Find: "查找",
  Replace: "替换",
  next: "下一个",
  previous: "上一个",
  all: "全部",
  "match case": "区分大小写",
  regexp: "正则",
  "by word": "全词",
  replace: "替换",
  "replace all": "全部替换",
  close: "关闭",
  "current match": "当前匹配",
  "on line": "第",
  "Go to line": "跳转到行",
  go: "跳转",
});

export interface PaneOptions {
  doc: string;
  path: string;
  readOnly: boolean;
  blame: Compartment;
  blameExtension: Extension;
  extraKeys?: KeyBinding[];
  extensions?: Extension[];
}

export function createPane(parent: HTMLElement, options: PaneOptions) {
  const language = new Compartment();
  const view = new EditorView({
    parent,
    state: EditorState.create({
      doc: options.doc,
      extensions: [
        phrases,
        options.blame.of(options.blameExtension),
        lineNumbers(),
        highlightActiveLineGutter(),
        history(),
        drawSelection(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        search({ top: true }),
        syntaxHighlighting(classHighlighter),
        language.of([]),
        marksField,
        EditorState.tabSize.of(4),
        EditorState.readOnly.of(options.readOnly),
        EditorView.editable.of(!options.readOnly),
        keymap.of([...(options.extraKeys ?? []), ...searchKeymap, ...historyKeymap, indentWithTab, ...defaultKeymap]),
        ...(options.extensions ?? []),
      ],
    }),
  });
  const description = LanguageDescription.matchFilename(languages, options.path.split("/").pop() ?? options.path);
  if (description) {
    description
      .load()
      .then((support) => {
        if (!view.dom.isConnected) return;
        view.dispatch({ effects: language.reconfigure(support) });
      })
      .catch(() => undefined);
  }
  return view;
}

export interface SideStates {
  ours: SideState;
  theirs: SideState;
}

export interface MergeSnapshot {
  ranges: LineRange[];
  states: SideStates[];
}

export const setMerge = StateEffect.define<MergeSnapshot>();

function mapRanges(ranges: LineRange[], tr: Transaction) {
  const before = tr.startState.doc;
  const after = tr.state.doc;
  const posOf = (line: number) => (line >= before.lines ? before.length : before.line(line + 1).from);
  return ranges.map((range) => {
    const start = after.lineAt(tr.changes.mapPos(posOf(range.start), 1)).number - 1;
    const end =
      range.end >= before.lines ? after.lines : after.lineAt(tr.changes.mapPos(posOf(range.end), -1)).number - 1;
    return { start: range.start >= before.lines ? after.lines : start, end: Math.max(start, end) };
  });
}

export function createMergeField(initial: MergeSnapshot) {
  const field = StateField.define<MergeSnapshot>({
    create: () => initial,
    update(value, tr) {
      for (const effect of tr.effects) if (effect.is(setMerge)) return effect.value;
      if (!tr.docChanged) return value;
      return { ranges: mapRanges(value.ranges, tr), states: value.states };
    },
  });
  const undoable = invertedEffects.of((tr) => {
    if (!tr.effects.some((effect) => effect.is(setMerge))) return [];
    return [setMerge.of(tr.startState.field(field))];
  });
  return { field, extension: [field, undoable] as Extension };
}

export function lineTop(view: EditorView, line: number) {
  const doc = view.state.doc;
  if (line >= doc.lines) return view.lineBlockAt(doc.length).bottom;
  return view.lineBlockAt(doc.line(Math.max(1, line + 1)).from).top;
}

export function topLine(view: EditorView, height: number) {
  const block = view.lineBlockAtHeight(Math.max(0, height));
  const line = view.state.doc.lineAt(block.from).number - 1;
  return line + Math.min(1, Math.max(0, (height - block.top) / Math.max(1, block.height)));
}

export function heightOfLine(view: EditorView, line: number) {
  const doc = view.state.doc;
  const whole = Math.floor(line);
  if (whole >= doc.lines) return view.lineBlockAt(doc.length).bottom;
  const block = view.lineBlockAt(doc.line(Math.max(1, whole + 1)).from);
  return block.top + (line - whole) * block.height;
}
