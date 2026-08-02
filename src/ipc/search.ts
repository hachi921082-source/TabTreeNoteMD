// Thin typed wrapper around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/search.rs` (`docs/plans/project-architecture/PLAN.md` §6;
// `docs/plans/search-index/PLAN.md` §4). No business logic here.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { CrossTabSearchOptions, SearchHit } from "../shared/models";

export async function searchQuery(query: string, options: CrossTabSearchOptions): Promise<Result<SearchHit[]>> {
  return invoke<Result<SearchHit[]>>("search_query", { query, options });
}
