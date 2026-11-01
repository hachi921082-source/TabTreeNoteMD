// Client-side mirror of `src-tauri/src/util/unique_name.rs`'s `resolve_unique_name` (GitHub issue
// #97's `base`, `base(1)`, `base(2)`, ... algorithm) — reused here purely to *preview* what a
// folder-import collision dialog's "Rename" choice would produce, so the dialog/summary toast can
// show the resulting name before any write happens (`docs/plans/v1.1.0/folder-import/PLAN.md` §2).
// The backend's own `resolve_unique_name` remains authoritative for the actual write (`import_file`
// never trusts this preview verbatim); this is the same "client-side mirror for instant feedback,
// server-side authoritative" split `shared/nameSanitize.ts` already established.

export function resolveUniqueName(base: string, exists: (candidate: string) => boolean): string {
  if (!exists(base)) return base;
  let n = 1;
  for (;;) {
    const candidate = `${base}(${n})`;
    if (!exists(candidate)) return candidate;
    n += 1;
  }
}
