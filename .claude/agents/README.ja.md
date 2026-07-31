# .claude/agents/

このプロジェクト専用のサブエージェント定義を置く場所です。
`agent-name.md`（frontmatterに `name` / `description` / `tools` / `model` を記述）の形式で追加します。

## 現在のエージェント

- `designer-lite` — 仕様書（`docs/spec.md`）の相談・作成アシスタント／レビュワー。developer-liteからの質問対応、詳細仕様書（`docs/detailed-spec.md`）の作成も担当。
- `developer-lite` — 仕様書を読み、機能ごとの実装計画（`docs/plans/<機能名>/PLAN.md`）を立てたうえでアプリ（`src/`）を実装する担当。
- `reviewer-lite` — 実装されたアプリを仕様書類と照らし合わせ、実際に起動・操作してテストする担当。

`developer-lite` → `reviewer-lite` の順に呼び出す2エージェント構成（worker/reviewer）で運用します。

このプロジェクトは個人用の小規模アプリのため、往復ラウンドを数える永続ログ（AGENT_LOG.md相当）は持たず、「一方向エスカレーション」（`docs/QA.md`に書いて止まる）で運用する軽量構成（"lite"セット）を採用しています。

## ワークフローの思想

Claude Code本体（オーケストレーター）がユーザーとサブエージェントの間、およびサブエージェント同士のやりとりを基本的に仲介します。サブエージェントは実行を終えると結果テキストを呼び出し元に返すだけで、ユーザーと直接会話する手段は持ちません。

例外として、**`developer-lite` だけ Agentツールを持たせています**（`designer-lite`・`reviewer-lite`は持っていません）。これにより、`developer-lite`は実装計画づくりの途中で`designer-lite`を直接呼び出して質問できます。往復ループを避けるため、この呼び出しは**`developer-lite` → `designer-lite` の一方向のみ**とし、`designer-lite`から`developer-lite`を呼び返すことはありません。

### 全体の流れ

1. **仕様のすり合わせ（ユーザー ⇔ designer-lite）**: ユーザーが`designer-lite`と直接、`docs/spec.md`を詰める。決定の経緯・却下案は`docs/DECISIONS.md`側に記録する。これがすべての土台。
2. **詳細仕様書の作成（designer-lite）**: `docs/spec.md`の確定内容をもとに、`designer-lite`が実装計画のベースとなる`docs/detailed-spec.md`を作成する。**`docs/spec.md`自体はこの工程では編集しない。**
3. **実装計画の作成（developer-lite ⇔ designer-lite）**: `developer-lite`が`docs/spec.md`・`docs/detailed-spec.md`を読み、機能ごとに独立した`docs/plans/<機能名>/PLAN.md`（関数仕様書等を含む）を作成する。旧TabTreeNoteのようなマイルストーン積み上げ順の単一ファイルにはしない。途中で疑問が出たら`developer-lite`が`designer-lite`を直接呼び出して確認する。
   - `designer-lite`が確定仕様の範囲内で答えられる場合はその場で回答。
   - `designer-lite`が「これはユーザー本人の判断が必要」と判断した場合は、自分で仮決めせず`docs/QA.md`に質問を追記し、そこでやりとりを終了。`developer-lite`はその論点の計画づくりを止め、ユーザーに「`docs/QA.md`を確認してほしい」と報告する。
4. **実装（developer-lite）**: 実装計画がまとまった機能から、`src/`への実装に着手する。
5. **レビュー（reviewer-lite）**: 実装後、`reviewer-lite`が`docs/spec.md`・`docs/detailed-spec.md`・該当`PLAN.md`との整合性をチェックし、実際にアプリを起動して主要フローを操作して確認する。コード編集・コミットは行わず、指摘のみ行う。

### なぜこの設計か
- サブエージェントは「ユーザーに確認してください」と指示されても、実行の途中で本当にユーザーに質問して待つ手段がない。これを守らずエージェントが「ユーザーが回答した」と誤って自己完結させてしまう問題を避けるため、**「答えられない・ユーザーの判断が必要」と思ったら`docs/QA.md`に書いて必ず止まる**というルールを明文化している。
- `docs/spec.md`は常にユーザーとの直接対話でのみ更新される正本とし、実装都合で書き換わらないようにする。詳細化・実装都合の解釈は`docs/detailed-spec.md`・`docs/plans/`側に分離する。
- ユーザーがコードを読めない前提のため、`reviewer-lite`による**実機での動作確認**が唯一の安全網となる（技術選定の理由は`docs/DECISIONS.md`参照）。
