# TabTreeNoteMD

## 旧リポジトリ（参照専用）

- パス: `c:\Users\m9112\vscode2\TabTreeNote`。**参照専用、Edit/Write/コミットは行わない。** 参照先の詳細は[docs/DECISIONS.md](docs/DECISIONS.md)参照。

## 本リポジトリのドキュメント

- [docs/spec.md](docs/spec.md) — 旧リポジトリの設計判断・事後変更ログを統合した要件ベースライン。作り直しの設計はここを出発点として検討する。
- [docs/DECISIONS.md](docs/DECISIONS.md) — 決定の経緯・比較検討・却下案・旧リポジトリの参照先詳細（普段は読まず、必要な時だけ参照）。

## ワークフロー

designer-lite → developer-lite → reviewer-lite の3エージェント体制で運用する（ユーザーがエージェント名を明示しなくても、該当する作業は必ず対応するエージェント経由で行う）。詳細（採用理由・ドキュメント構成・エスカレーション方式）は[.claude/agents/README.md](.claude/agents/README.md)参照。
