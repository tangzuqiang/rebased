import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  LanguageDescription,
  bracketMatching,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { highlightSelectionMatches, search, searchKeymap } from "@codemirror/search";
import { Annotation, Compartment, EditorState, type Extension } from "@codemirror/state";
import {
  EditorView,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { classHighlighter } from "@lezer/highlight";

export interface IdeEditorOptions {
  parent: HTMLElement;
  doc?: string;
  path?: string;
  readOnly?: boolean;
  onChange?: (content: string) => void;
  onSave?: (content: string) => void;
  extensions?: Extension[];
}

export interface IdeEditor {
  readonly view: EditorView;
  getDoc(): string;
  setDoc(doc: string): void;
  setPath(path: string): void;
  setReadOnly(readOnly: boolean): void;
  update(options: { doc?: string; path?: string; readOnly?: boolean }): void;
  goTo(line: number, column?: number): void;
  focus(): void;
  destroy(): void;
}

const externalUpdate = Annotation.define<boolean>();

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

export function createIdeEditor(options: IdeEditorOptions): IdeEditor {
  const language = new Compartment();
  const editing = new Compartment();
  let path = options.path ?? "";
  let languageRequest = 0;
  let destroyed = false;

  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.doc ?? "",
      extensions: [
        phrases,
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
        EditorState.tabSize.of(4),
        language.of([]),
        editing.of(editingExtensions(Boolean(options.readOnly))),
        keymap.of([
          {
            key: "Mod-s",
            preventDefault: true,
            run: (current) => {
              options.onSave?.(current.state.doc.toString());
              return true;
            },
          },
          ...searchKeymap,
          ...historyKeymap,
          indentWithTab,
          ...defaultKeymap,
        ]),
        EditorView.updateListener.of((update) => {
          if (
            update.docChanged &&
            !update.transactions.some((transaction) => transaction.annotation(externalUpdate))
          ) {
            options.onChange?.(update.state.doc.toString());
          }
        }),
        ...(options.extensions ?? []),
      ],
    }),
  });

  function loadLanguage(filename: string) {
    const request = ++languageRequest;
    const name = filename.replace(/\\/g, "/").split("/").pop() ?? filename;
    const description = LanguageDescription.matchFilename(languages, name);
    if (!description) {
      view.dispatch({ effects: language.reconfigure([]) });
      return;
    }
    void description
      .load()
      .then((support) => {
        if (destroyed || request !== languageRequest) return;
        view.dispatch({ effects: language.reconfigure(support) });
      })
      .catch(() => undefined);
  }

  function setDoc(doc: string) {
    if (destroyed || doc === view.state.doc.toString()) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: doc },
      annotations: externalUpdate.of(true),
    });
  }

  function setPath(nextPath: string) {
    if (destroyed || nextPath === path) return;
    path = nextPath;
    loadLanguage(path);
  }

  function setReadOnly(readOnly: boolean) {
    if (destroyed) return;
    view.dispatch({ effects: editing.reconfigure(editingExtensions(readOnly)) });
  }

  loadLanguage(path);

  return {
    view,
    getDoc: () => view.state.doc.toString(),
    setDoc,
    setPath,
    setReadOnly,
    update(next) {
      if (next.doc !== undefined) setDoc(next.doc);
      if (next.path !== undefined) setPath(next.path);
      if (next.readOnly !== undefined) setReadOnly(next.readOnly);
    },
    goTo(line, column = 1) {
      if (destroyed) return;
      const lineInfo = view.state.doc.line(
        Math.min(Math.max(1, Math.trunc(line || 1)), view.state.doc.lines),
      );
      const position = Math.min(
        lineInfo.to,
        lineInfo.from + Math.max(0, Math.trunc(column || 1) - 1),
      );
      view.dispatch({
        selection: { anchor: position },
        effects: EditorView.scrollIntoView(position, { y: "center" }),
      });
      view.focus();
    },
    focus: () => view.focus(),
    destroy() {
      if (destroyed) return;
      destroyed = true;
      languageRequest += 1;
      view.destroy();
    },
  };
}

function editingExtensions(readOnly: boolean): Extension {
  return [
    EditorState.readOnly.of(readOnly),
    EditorView.editable.of(!readOnly),
  ];
}
