import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev-server port and a relative build output path so the
// bundled webview can load assets without an absolute file:// base.
// https://v2.tauri.app/start/frontend/vite/
const host = process.env.TAURI_DEV_HOST;

// 公開用ビルドとみなすデフォルトブランチ名。将来 `main` などに改名したらここだけ直す。
// このブランチかつ未コミット変更なしのときだけ、ラベルは版番号のみになる。
export const DEFAULT_BRANCH = "V1.0.0";

type GitRunner = (args: string) => string;

const runGit: GitRunner = (args) =>
  execSync(`git ${args}`, { stdio: ["ignore", "pipe", "ignore"], timeout: 5000 }).toString().trim();

// 設定画面に出す版表示を組み立てる(GitHub issue #323)。
// 例: `v1.2.0` / `v1.2.0 (fix/foo, 1a2b3c4)` / `v1.2.0 (fix/foo, 1a2b3c4, dirty)`。
// Git が使えない場合は版番号のみ。detached HEAD はブランチ名を省きコミットIDのみ添える。
export function buildVersionLabel(version: string, git: GitRunner = runGit): string {
  const base = `v${version}`;
  try {
    const branch = git("rev-parse --abbrev-ref HEAD");
    const commit = git("rev-parse --short HEAD");
    const dirty = git("status --porcelain --untracked-files=no") !== "";
    if (branch === DEFAULT_BRANCH && !dirty) return base;
    const parts = [branch === "HEAD" ? "" : branch, commit, dirty ? "dirty" : ""].filter(Boolean);
    return `${base} (${parts.join(", ")})`;
  } catch {
    return base;
  }
}

const pkgVersion: string = JSON.parse(
  readFileSync(new URL("./package.json", import.meta.url), "utf-8"),
).version;

export default defineConfig({
  plugins: [react()],
  define: {
    __APP_VERSION_LABEL__: JSON.stringify(buildVersionLabel(pkgVersion)),
  },

  // Prevent Vite from obscuring Rust errors.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // Tell Vite to ignore watching `src-tauri`.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    outDir: "dist",
  },
});
