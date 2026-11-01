# TabTreeNoteMD

**日本語** | [English](#english)

「タブ × ツリー」でローカルのメモを階層管理する、Windows 向けの Markdown メモエディタです。タブは1つのフォルダ、メモは1つの `.md` ファイルに対応します。メモはすべてプレーンな Markdown ファイルとして手元のフォルダに残るため、他のエディタやバックアップ、同期ツールとそのまま併用できます。

フリーソフト「CatMemoNote」の考え方(タブ=フォルダ、メモ=テキストファイル、3ペイン構成、ドラッグ&ドロップ操作、レジストリに依存しないポータブル設計)を、独立した実装として作り直したものです。

> **開発状況**: 開発中です(現在のバージョンは v1.2.0 の開発ブランチ)。Windows 11 を優先対象としています。仕様や挙動は今後変わる可能性があります。

## 主な機能

- **タブ × ツリー管理**: フォルダをタブとして登録し、中のメモをツリーで階層表示。ドラッグ&ドロップで並べ替え・移動(タブをまたぐ移動にも対応)
- **Markdown 編集**: ライブ表示(書式をその場で反映)とソース表示、プレビュー、左右または上下の分割表示。書式ツールバー、画像の貼り付け、数式(TeX、プレビュー)、目次、チェックボックスの切り替え
- **検索・置換**: メモ内の検索と置換、全タブ横断の全文検索
- **リンク**: URL・メール・ファイルパスの自動リンク、`memo:` 記法によるメモ間リンク、見出しへのアンカーリンク。日本語を含む URL・メールにも対応。右クリックで「リンクをコピー」
- **タブロック**: パスワードでタブ内のメモをまとめて暗号化(scrypt + AES-256-GCM)。ロック中のタブは検索の対象外
- **安全な保存**: アトミック書き込みと `.bak`、ゴミ箱への退避、外部での変更の検知と競合の確認
- **ポータブル**: 設定は実行ファイルの隣の `data/` フォルダに保存。レジストリを使わず、実行ファイルをコピーするだけで独立した環境になります
- **そのほか**: ライト/ダークテーマ、日本語/英語の表示切り替え、クイックオープン、プレビューのズーム、PDF 出力、トレイ常駐と自動起動、多重起動の防止

## 動作環境

- Windows 11(優先対象)
- WebView2 ランタイム(Windows 11 には標準で入っています)

## ソースからのビルド

必要なもの: [Node.js](https://nodejs.org/)、[Rust](https://www.rust-lang.org/)(1.77.2 以上)、[Tauri の前提条件](https://v2.tauri.app/start/prerequisites/)(Windows では Microsoft C++ Build Tools と WebView2)。

```sh
npm install

# 開発モードで起動
npm run tauri dev

# 本番ビルド(実行ファイルのみ)
npm run tauri build -- --no-bundle
# 出力: src-tauri/target/release/tabtreenotemd.exe
```

技術構成: Tauri 2(Rust)+ React + TypeScript + CodeMirror 6。プレビューは markdown-it と DOMPurify で生成・サニタイズし、数式は KaTeX で表示します。

## ドキュメント

仕様や設計の経緯は [docs/](docs/README.md) にあります。

- [docs/spec.md](docs/spec.md) — v1.0.0 の確定仕様
- [docs/spec.v1.1.0.md](docs/spec.v1.1.0.md) / [docs/spec.v1.2.0.md](docs/spec.v1.2.0.md) — バージョンごとの追加・変更仕様
- [docs/RESOLVED-ISSUES.md](docs/RESOLVED-ISSUES.md) — 解決済みの issue の記録

## ライセンスと第三者ソフトウェア

ライセンスは未定です(公開前に確定します)。[EULA.md](EULA.md) は現時点ではたたき台のドラフトで、確定したものではありません。

本アプリは多数のオープンソースのコンポーネントを利用しています。一覧とライセンスは [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) を参照してください。

---

<a id="english"></a>

# TabTreeNoteMD (English)

[日本語](#tabtreenotemd) | **English**

A Markdown note editor for Windows that manages local notes in a "tabs × tree" layout. A tab maps to a folder and a memo maps to a single `.md` file. Every memo stays a plain Markdown file in a folder you control, so it works alongside other editors, backup tools and sync tools.

It is an independent re-implementation of the ideas of the freeware "CatMemoNote": tabs = folders, memos = text files, a three-pane layout, drag-and-drop operations, and a registry-free portable design.

> **Status**: under development (the current branch is the v1.2.0 line). Windows 11 is the primary target. Specifications and behavior may change.

## Features

- **Tabs × tree**: register folders as tabs and browse their memos as a tree. Reorder and move with drag and drop, including moves across tabs
- **Markdown editing**: live view (formatting applied in place), source view, preview, and a split view (side by side or stacked). Formatting toolbar, image paste, math (TeX, in the preview), table of contents, checkbox toggling
- **Search and replace**: in-memo search and replace, plus full-text search across all tabs
- **Links**: automatic links for URLs, emails and file paths, `memo:` links between memos, heading anchor links. URLs and emails containing Japanese and other non-ASCII text are supported. "Copy link" in the right-click menu
- **Tab lock**: encrypt all memos in a tab with a password (scrypt + AES-256-GCM). Locked tabs are excluded from search
- **Safe saving**: atomic writes with `.bak` files, a trash folder, and detection of outside changes with conflict prompts
- **Portable**: settings live in a `data/` folder next to the executable. No registry use; copying the executable gives you an independent environment
- **Also**: light/dark theme, Japanese/English UI, quick open, preview zoom, PDF export, tray residence with autostart, single-instance guard

## Requirements

- Windows 11 (primary target)
- WebView2 runtime (included with Windows 11)

## Building from source

You need [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/) (1.77.2 or later), and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Microsoft C++ Build Tools and WebView2 on Windows).

```sh
npm install

# run in development mode
npm run tauri dev

# production build (executable only)
npm run tauri build -- --no-bundle
# output: src-tauri/target/release/tabtreenotemd.exe
```

Stack: Tauri 2 (Rust) + React + TypeScript + CodeMirror 6. The preview is rendered with markdown-it and sanitized with DOMPurify; math is rendered with KaTeX.

## Documentation

Specifications and design history are in [docs/](docs/README.md).

- [docs/spec.md](docs/spec.md) — the finalized v1.0.0 specification
- [docs/spec.v1.1.0.md](docs/spec.v1.1.0.md) / [docs/spec.v1.2.0.md](docs/spec.v1.2.0.md) — additions and changes per version
- [docs/RESOLVED-ISSUES.md](docs/RESOLVED-ISSUES.md) — record of resolved issues

## License and third-party software

The license is not decided yet (it will be settled before publication). [EULA.md](EULA.md) is currently a draft starting point, not a final document.

This app uses many open-source components. See [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for the list and their licenses.
