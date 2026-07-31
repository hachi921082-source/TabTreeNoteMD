# docs/

このディレクトリには仕様書類を置きます（機能仕様、画面構成、アーキテクチャ検討メモなど）。

## ファイル構成

- `spec.md` — 確定仕様書。ユーザーと`designer-lite`が直接詰める、機能仕様の正本。「今何が決まっているか」だけを簡潔に記す。
- `DECISIONS.md` — 決定の経緯・比較検討・却下案。`spec.md`を簡潔に保つため、詳しい背景はこちらに集約する。
- `detailed-spec.md` — 詳細仕様書。`designer-lite`が`spec.md`の確定内容をもとに、`developer-lite`が実装計画を立てられるレベルまで詳細化したもの。
- `plans/<機能名>/PLAN.md` — 機能ごとに独立した実装計画。`developer-lite`が`spec.md`・`detailed-spec.md`をもとに作成する、関数仕様書等を含む実装の設計文書。旧TabTreeNoteのようなマイルストーン積み上げ順にはしない。
- `QA.md` — `developer-lite`が実装計画づくりの途中で`designer-lite`に確認し、「ユーザー本人の決定が必要」と判断された質問を集めたもの。ユーザーはここを確認して回答する。
