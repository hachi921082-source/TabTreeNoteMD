// Pure tree helper functions (find/locate node, etc.) — anticipated by
// `docs/plans/project-architecture/PLAN.md` §2's directory layout
// ("shared/treeOps.ts"). Used by `components/Tree/` for DND drop-position
// resolution (`docs/plans/memo-tree/PLAN.md` §4) without needing to walk
// `TreeNode[]` inline in component code.
import type { TreeNode } from "./models";

export interface NodeLocation {
  node: TreeNode;
  /** `memoName` of the containing node, or `null` if this is a root-level node. */
  parentPath: string | null;
  /** Index within its parent's (or the root list's) children array. */
  index: number;
}

/** Depth-first search for `memoName`, returning its node plus its parent path and index within
 * its sibling list. `null` if not found anywhere in the tree. */
export function locateNode(nodes: readonly TreeNode[], memoName: string, parentPath: string | null = null): NodeLocation | null {
  for (let i = 0; i < nodes.length; i++) {
    const n = nodes[i];
    if (n.memoName === memoName) return { node: n, parentPath, index: i };
    const found = locateNode(n.children, memoName, n.memoName);
    if (found) return found;
  }
  return null;
}

export function findNode(nodes: readonly TreeNode[], memoName: string): TreeNode | null {
  return locateNode(nodes, memoName)?.node ?? null;
}

/** `memoName` plus every descendant's `memoName`, pre-order. Used to reject (in the UI, before
 * even calling the backend, which independently rejects it too) dropping a node onto one of its
 * own descendants. */
export function subtreeNames(node: TreeNode): string[] {
  const out: string[] = [node.memoName];
  for (const child of node.children) out.push(...subtreeNames(child));
  return out;
}
