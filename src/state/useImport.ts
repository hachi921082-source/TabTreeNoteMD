// Collision-detection/resolution state for OS-folder -> tree-pane drag-and-drop import
// (`docs/plans/v1.1.0/folder-import/PLAN.md` §4). Runs entirely client-side against the active
// tab's already-in-memory `TreeNode[]`, then calls `import_file` once per already-resolved item,
// sequentially — the "client-orchestrated collision loop, backend executes one resolved item at a
// time" design (`PLAN.md` §2). No new read-only round-trip command is introduced for the collision
// check itself.
import { useCallback, useRef, useState } from "react";
import { importFile } from "../ipc/import";
import { resolveUniqueName } from "../shared/uniqueName";
import { suggestSanitized } from "../shared/nameSanitize";
import type { TreeNode } from "../shared/models";
import type { ImportCollisionChoice } from "../components/dialogs/ImportCollisionDialog";
import type { UseToastResult } from "./useToast";
import { useT } from "../i18n";
import { errorMessageFor } from "../shared/errorMessages";

export interface PendingImportCollision {
  sourceName: string;
  renamePreview: string;
  overwriteDisabled: boolean;
}

export interface UseImportResult {
  /** Non-`null` while `ImportCollisionDialog` should be shown for the current colliding item. */
  pendingCollision: PendingImportCollision | null;
  /** `ImportCollisionDialog`'s Rename/Overwrite/Skip choice, plus the "apply to all" checkbox. */
  resolveCollision: (choice: ImportCollisionChoice, applyToAll: boolean) => void;
  /** Esc / backdrop-close on the collision dialog — stops the whole remaining batch (`PLAN.md`
   * §4's "this item and every remaining not-yet-processed item are tallied as local skips"). */
  cancelImport: () => void;
  /** `true` while a batch (collision resolution and/or the sequential `import_file` calls) is in
   * progress — `components/Tree/TreeView.tsx`'s OS drop listener ignores a second drop landing
   * while this is `true` (`PLAN.md` §4: "no other tree operation ... is accepted" while the
   * dialog is open; extended here to cover the whole batch, not just the dialog-open moments). */
  importing: boolean;
  /** Kicks off an import batch for `sourcePaths` (already `.md`-filtered by the caller) dropped
   * onto `parent` (`null` = root level's end). `nonMdSkipped` is the count of non-`.md` items in
   * the original drop payload (folders included), folded into the summary toast's skip tally. */
  startImportBatch: (sourcePaths: string[], parent: string | null, nonMdSkipped: number) => void;
}

interface QueuedItem {
  sourcePath: string;
  targetName: string;
  overwrite: boolean;
}

type CollisionResolution = { choice: ImportCollisionChoice; applyToAll: boolean } | "cancel";

/** Extension-stripped base name from an OS file path (`PLAN.md` §4 step 1) — handles both `\` and
 * `/` separators since the dropped path always comes from the native OS payload (Windows paths use
 * `\`, but a defensive `/` split costs nothing). */
function baseNameOf(sourcePath: string): string {
  const normalized = sourcePath.replace(/\\/g, "/");
  const withExt = normalized.split("/").pop() ?? sourcePath;
  return withExt.replace(/\.md$/i, "");
}

function collectNamesLower(node: TreeNode, out: Set<string>) {
  out.add(node.memoName.toLowerCase());
  for (const child of node.children) collectNamesLower(child, out);
}

/** Case-insensitive counterpart to `treeOps.ts`'s `findNode` — used only for the collision-target
 * Edit-Lock lookup below, to stay consistent with `claimedNames`'s lowercased collision-detection
 * convention and the backend's `find_node_ci` (`import.rs`/`tab_lock.rs`). `findNode` itself stays
 * exact-case since other callers rely on that. */
function findNodeCI(nodes: readonly TreeNode[], lowerName: string): TreeNode | null {
  for (const node of nodes) {
    if (node.memoName.toLowerCase() === lowerName) return node;
    const found = findNodeCI(node.children, lowerName);
    if (found) return found;
  }
  return null;
}

export function useImport(tabId: string | null, nodes: TreeNode[], refreshTree: () => Promise<void>, toast: UseToastResult): UseImportResult {
  const t = useT();
  const [pendingCollision, setPendingCollision] = useState<PendingImportCollision | null>(null);
  const [importing, setImporting] = useState(false);
  const resolverRef = useRef<((r: CollisionResolution) => void) | null>(null);
  const batchPolicyRef = useRef<ImportCollisionChoice | null>(null);

  const resolveCollision = useCallback((choice: ImportCollisionChoice, applyToAll: boolean) => {
    setPendingCollision(null);
    resolverRef.current?.({ choice, applyToAll });
    resolverRef.current = null;
  }, []);

  const cancelImport = useCallback(() => {
    setPendingCollision(null);
    resolverRef.current?.("cancel");
    resolverRef.current = null;
  }, []);

  const askUser = useCallback((sourceName: string, renamePreview: string, overwriteDisabled: boolean): Promise<CollisionResolution> => {
    setPendingCollision({ sourceName, renamePreview, overwriteDisabled });
    return new Promise((resolve) => {
      resolverRef.current = resolve;
    });
  }, []);

  const startImportBatch = useCallback(
    (sourcePaths: string[], parent: string | null, nonMdSkipped: number) => {
      if (!tabId || sourcePaths.length === 0) {
        // Every path in the payload was non-`.md`: still shows the summary toast (not the fully-
        // silent zero-`.md`-items path, `PLAN.md` §6) — but only if there was anything to skip at
        // all (an empty payload, e.g. a stray drag-over with no drop, reports nothing).
        if (nonMdSkipped > 0) toast.pushToast(t("toast.importZeroSkippedTemplate").replace("{count}", String(nonMdSkipped)));
        return;
      }

      setImporting(true);
      batchPolicyRef.current = null;

      void (async () => {
        const claimedNames = new Set<string>();
        for (const node of nodes) collectNamesLower(node, claimedNames);

        const queued: QueuedItem[] = [];
        let skipped = nonMdSkipped;
        let cancelled = false;

        for (const sourcePath of sourcePaths) {
          if (cancelled) {
            skipped += 1;
            continue;
          }

          const sanitizedName = suggestSanitized(baseNameOf(sourcePath));
          const lowerName = sanitizedName.toLowerCase();

          if (!claimedNames.has(lowerName)) {
            queued.push({ sourcePath, targetName: sanitizedName, overwrite: false });
            claimedNames.add(lowerName);
            continue;
          }

          const collisionNode = findNodeCI(nodes, lowerName);
          const overwriteDisabled = collisionNode?.locked === true;

          let choice: ImportCollisionChoice;
          const policy = batchPolicyRef.current;
          if (policy) {
            choice = policy;
          } else {
            const renamePreview = resolveUniqueName(sanitizedName, (c) => claimedNames.has(c.toLowerCase()));
            const resolution = await askUser(sanitizedName, renamePreview, overwriteDisabled);
            if (resolution === "cancel") {
              cancelled = true;
              skipped += 1;
              continue;
            }
            choice = resolution.choice;
            if (resolution.applyToAll) batchPolicyRef.current = resolution.choice;
          }

          // Silent force-conversion: an "apply to all: overwrite" policy hitting an edit-locked
          // collision target is converted to Skip without ever showing a dialog for that item
          // (`PLAN.md` §4).
          if (choice === "overwrite" && overwriteDisabled) {
            choice = "skip";
          }

          if (choice === "skip") {
            skipped += 1;
          } else if (choice === "overwrite") {
            queued.push({ sourcePath, targetName: sanitizedName, overwrite: true });
          } else {
            const resolvedName = resolveUniqueName(sanitizedName, (c) => claimedNames.has(c.toLowerCase()));
            queued.push({ sourcePath, targetName: resolvedName, overwrite: false });
            claimedNames.add(resolvedName.toLowerCase());
          }
        }

        let imported = 0;
        // 見つからなかった相対画像の件数(ドロップ1回分の合計、GitHub issue #265)
        let missingImages = 0;
        // 長いパスになった取り込みがあったか(ドロップ1回分をまとめて1回だけ警告、GitHub issue #319)
        let longPath = false;
        for (const item of queued) {
          // Sequential, awaited one at a time — so a later item's defensive backend re-check sees
          // the previous item's already-committed state (`PLAN.md` §4 step 3).
          const result = await importFile(tabId, parent, item.sourcePath, item.targetName, item.overwrite);
          if (result.ok) {
            imported += 1;
            missingImages += result.value.missingImages;
            if (result.value.longPath) longPath = true;
          } else skipped += 1;
        }

        // No selection/scroll-position side effects (`PLAN.md` §4 step 5) — `refreshTree` only
        // updates `nodes`/`tabName`/`locked`/`recoveryPending`/`recoveryKind` (`state/useTree.ts`'s
        // `refresh`), never `openMemoName`/selection.
        if (imported > 0) await refreshTree();

        setImporting(false);
        if (skipped > 0) {
          toast.pushToast(t("toast.importSummaryTemplate").replace("{imported}", String(imported)).replace("{skipped}", String(skipped)));
        }
        if (missingImages > 0) {
          toast.pushToast(t("toast.importMissingImagesTemplate").replace("{count}", String(missingImages)));
        }
        if (longPath) {
          toast.pushToast(errorMessageFor("LONG_PATH", t), "warning");
        }
      })();
    },
    [tabId, nodes, toast, refreshTree, askUser, t],
  );

  return { pendingCollision, resolveCollision, cancelImport, importing, startImportBatch };
}
