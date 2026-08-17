// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/search.rs` (`docs/plans/project-architecture/PLAN.md` §6;
// `docs/plans/search-index/PLAN.md` §4). No business logic here.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { CrossTabSearchOptions, QuickOpenHit, SearchHit } from "../shared/models";

export async function searchQuery(query: string, options: CrossTabSearchOptions): Promise<Result<SearchHit[]>> {
  return invoke<Result<SearchHit[]>>("search_query", { query, options });
}

/** Cross-Tab Quick Open (`docs/plans/v1.1.0/quick-open/PLAN.md` §2) — matches memo display names
 * only, across every unlocked tab. `query: ""` returns every candidate (Q3-1), not an empty list. */
export async function searchQuickOpen(query: string): Promise<Result<QuickOpenHit[]>> {
  return invoke<Result<QuickOpenHit[]>>("search_quick_open", { query });
}

/** Cross-Tab Favorites list (`docs/plans/v1.1.0/favorites/PLAN.md` §5) — every favorited,
 * index-eligible memo across every unlocked tab, no query input and no result cap (unlike
 * `searchQuickOpen` above). Reuses `QuickOpenHit`; `isFavorite` is always `true` per row. */
export async function searchGetFavorites(): Promise<Result<QuickOpenHit[]>> {
  return invoke<Result<QuickOpenHit[]>>("search_get_favorites");
}
