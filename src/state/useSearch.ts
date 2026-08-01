// In-memo search & replace domain state (`docs/plans/editor-save/PLAN.md` §5). Wraps
// `@codemirror/search`'s primitives against the currently-mounted `EditorView`. The UI itself
// (`components/Search/SearchPanel.tsx`) is a custom component — not `@codemirror/search`'s
// built-in DOM panel (`src/editor/textCommands.ts` only registers the `search()` extension's
// state field, never calls `openSearchPanel`) — so this hook also owns the search-string history
// ring buffer. **Confirmed session-only** (in-memory, cleared on app restart, never persisted to
// `AppSettings` or anywhere else — `[detailed-spec §5.4]`, `DECISIONS.md` "UI-State Persistence
// Decisions").
import { useCallback, useMemo, useState } from "react";
import type { EditorView } from "@codemirror/view";
import { SearchQuery, findNext, findPrevious, replaceAll as cmReplaceAll, replaceNext, selectMatches, setSearchQuery } from "@codemirror/search";

const HISTORY_LIMIT = 20;

export interface SearchOptions {
  caseSensitive: boolean;
  regexp: boolean;
  wholeWord: boolean;
}

export interface UseSearchResult {
  isOpen: boolean;
  open: () => void;
  close: () => void;
  searchText: string;
  replaceText: string;
  options: SearchOptions;
  /** Most-recent-first, capped at `HISTORY_LIMIT`, de-duplicated. */
  history: string[];
  setSearchText: (v: string) => void;
  setReplaceText: (v: string) => void;
  toggleOption: (key: keyof SearchOptions) => void;
  findNextInView: (view: EditorView) => void;
  findPreviousInView: (view: EditorView) => void;
  replaceOneInView: (view: EditorView) => void;
  replaceAllInView: (view: EditorView) => void;
  selectAllMatchesInView: (view: EditorView) => void;
}

export function useSearch(): UseSearchResult {
  const [isOpen, setIsOpen] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [replaceText, setReplaceText] = useState("");
  const [options, setOptions] = useState<SearchOptions>({ caseSensitive: false, regexp: false, wholeWord: false });
  const [history, setHistory] = useState<string[]>([]);

  const open = useCallback(() => setIsOpen(true), []);
  const close = useCallback(() => setIsOpen(false), []);

  const pushHistory = useCallback((text: string) => {
    if (!text) return;
    setHistory((current) => [text, ...current.filter((h) => h !== text)].slice(0, HISTORY_LIMIT));
  }, []);

  const toggleOption = useCallback((key: keyof SearchOptions) => {
    setOptions((current) => ({ ...current, [key]: !current[key] }));
  }, []);

  const query = useMemo(
    () =>
      new SearchQuery({
        search: searchText,
        replace: replaceText,
        caseSensitive: options.caseSensitive,
        regexp: options.regexp,
        wholeWord: options.wholeWord,
      }),
    [searchText, replaceText, options],
  );

  const syncAndRecord = useCallback(
    (view: EditorView) => {
      view.dispatch({ effects: setSearchQuery.of(query) });
      pushHistory(searchText);
    },
    [query, pushHistory, searchText],
  );

  const findNextInView = useCallback(
    (view: EditorView) => {
      syncAndRecord(view);
      findNext(view);
    },
    [syncAndRecord],
  );

  const findPreviousInView = useCallback(
    (view: EditorView) => {
      syncAndRecord(view);
      findPrevious(view);
    },
    [syncAndRecord],
  );

  const replaceOneInView = useCallback(
    (view: EditorView) => {
      syncAndRecord(view);
      replaceNext(view);
    },
    [syncAndRecord],
  );

  const replaceAllInView = useCallback(
    (view: EditorView) => {
      syncAndRecord(view);
      cmReplaceAll(view);
    },
    [syncAndRecord],
  );

  const selectAllMatchesInView = useCallback(
    (view: EditorView) => {
      syncAndRecord(view);
      selectMatches(view);
    },
    [syncAndRecord],
  );

  return {
    isOpen,
    open,
    close,
    searchText,
    replaceText,
    options,
    history,
    setSearchText,
    setReplaceText,
    toggleOption,
    findNextInView,
    findPreviousInView,
    replaceOneInView,
    replaceAllInView,
    selectAllMatchesInView,
  };
}
