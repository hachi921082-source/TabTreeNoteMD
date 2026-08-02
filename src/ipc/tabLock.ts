// Thin typed wrappers around `@tauri-apps/api`'s `invoke`, mirroring
// `src-tauri/src/commands/tab_lock.rs` (`docs/plans/project-architecture/PLAN.md` §6;
// `docs/plans/tab-lock/PLAN.md` §3; `docs/plans/tab-lock/CRASH-RECOVERY.md`). No business logic
// here; validation/branching on the `Result` happens in `state/useTabLock.ts` or the calling
// component.
import { invoke } from "@tauri-apps/api/core";
import type { Result } from "../shared/result";
import type { RecoveryStatus, TabInfo } from "../shared/models";

/** `hint` omitted (or `undefined`) means no hint was set for this lock. */
export async function tabLock(tabId: string, password: string, hint?: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_lock", { tabId, password, hint: hint ?? null });
}

export async function tabUnlock(tabId: string, password: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_unlock", { tabId, password });
}

/** Triggered the moment the user switches away from a tab with an active unlock session
 * (`tab-lock/PLAN.md` §3 — no idle timeout, no manual "lock now" button). */
export async function tabEndSession(tabId: string): Promise<Result<void>> {
  return invoke<Result<void>>("tab_end_session", { tabId });
}

/** Fetches the crash-mid-lock recovery classification for a tab currently reporting
 * `recoveryPending: true` (`tree_open`'s `TreeSnapshot`). */
export async function tabLockRecoveryStatus(tabId: string): Promise<Result<RecoveryStatus>> {
  return invoke<Result<RecoveryStatus>>("tab_lock_recovery_status", { tabId });
}

export async function tabLockRecoveryFinish(tabId: string, password: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_lock_recovery_finish", { tabId, password });
}

export async function tabLockRecoveryRestore(tabId: string, password: string): Promise<Result<TabInfo>> {
  return invoke<Result<TabInfo>>("tab_lock_recovery_restore", { tabId, password });
}
