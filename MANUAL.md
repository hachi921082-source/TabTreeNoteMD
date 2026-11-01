<a id="japanese"></a>

# TabTreeNoteMD 操作ガイド

**日本語** | [English](#english)

このガイドは、TabTreeNoteMD をはじめて使う方のための操作説明です。プログラミングの知識は不要です。画面上の名前(メニュー名・ボタン名)は「」で示します。

> バージョンによって画面の文言や挙動が変わることがあります。このガイドは v1.2.0 開発ブランチの内容に基づいています。

## 目次

1. [はじめに](#1-はじめに)
2. [タブを管理する](#2-タブを管理する)
3. [メモを管理する](#3-メモを管理する)
4. [メモを編集する](#4-メモを編集する)
5. [検索と置換](#5-検索と置換)
6. [リンク](#6-リンク)
7. [タブロック](#7-タブロック)
8. [設定](#8-設定)
9. [ショートカット一覧](#9-ショートカット一覧)
10. [困ったとき](#10-困ったとき)

---

## 1. はじめに

### 1.1 基本の考え方

TabTreeNoteMD は、メモを「タブ × ツリー」で整理する Windows 向けの Markdown メモアプリです。

- **タブ = フォルダ**: 画面上部のタブ1つが、パソコン上のフォルダ1つに対応します。
- **メモ = `.md` ファイル**: タブの中のメモ1つが、そのフォルダ直下の Markdown ファイル(拡張子 `.md`)1つに対応します。メモの名前とファイル名は同じです。
- **ツリー**: 画面左側に、選択中のタブのメモが親子関係(階層)つきで表示されます。
- **エディタ**: 画面右側で、選んだメモの内容を編集します。

メモは普通のテキストファイルとして手元のフォルダに残るので、ほかのエディタで開いたり、フォルダごとコピーしてバックアップしたりできます。

> 注意: ツリーに表示されるのは、タブのフォルダ**直下**の `.md` ファイルだけです。フォルダの中のサブフォルダや、`.md` 以外のファイルは表示されません。

### 1.2 画面の構成

- **上部のタブバー**: 左から、アプリ名、各タブ、「既存フォルダを登録」ボタン(📂)、「新しいタブを作成」ボタン(＋)が並びます。右側にツールボタン(★ お気に入り、📌 最前面に固定、🔍 検索、⇄ ツリーの位置を切り替え、⚙ 設定)と、最小化・最大化・閉じるのボタンがあります。
- **左のツリー**: メモの一覧です。
- **右のエディタ**: メモの編集画面です。画面下部のステータスバーに、文字数・行数・カーソル位置(「○ 行, ○ 列」)が表示されます。
- **ツールバー**: エディタの上に浮かぶ書式ボタンの一群です(4.3 参照)。

ツリーとエディタの境目をドラッグすると、ツリーの幅を変えられます。境目をダブルクリックするとツリーを隠せます。隠したあとは「ツリーを表示」で元に戻せます。

このアプリには Windows 標準のタイトルバーがありません。タブバーの空いている部分をドラッグするとウィンドウを移動できます。

右上の「閉じる」ボタン(×)を押すと、アプリは**終了します**(トレイには入りません)。終了の直前に、編集中のメモは保存されます。設定による動作の切り替えはありません。最小化したときはタスクバーに残ります。タスクトレイにアイコンが出ますが、トレイに入った状態になるのは「Windowsログイン時にトレイへ自動起動する」で起動した場合だけです(8.1 参照)。

### 1.3 データの保存場所(ポータブル構成)

- アプリの設定とタブの登録情報は、実行ファイルと同じ場所にある **`data/` フォルダ**に保存されます(Windows のレジストリや `%APPDATA%` は使いません)。
- 実行ファイルだけを別の場所にコピーして起動すると、設定もタブも空の、独立した環境として動きます。複数のコピーを並べて使うこともできます。
- 「新しいタブを作成」で作ったタブのフォルダは、`data/tabs/` の下に作られます。「既存フォルダを登録」で登録したフォルダは、元の場所のまま使われます。
- 設定ごと別のパソコンへ移したいときは、実行ファイルと `data/` フォルダをいっしょにコピーしてください。

### 1.4 自動保存と再開

- 編集した内容は自動で保存されます。ほかに、タブやメモを切り替えたとき、最小化したとき、終了するとき、Ctrl+S を押したときにも保存されます。自動保存の間隔は「設定」で変えられます(8 参照)。
- 保存のたびに、直前の内容が `メモ名.md.bak` というファイルに1世代だけ残ります(10.2 参照)。
- 起動すると、最後に開いていたメモが自動で開きます。タブを切り替えたときも、そのタブで最後に開いていたメモが開き、スクロール位置も復元されます(そのメモがすでに無い場合は何も開きません)。

### 1.5 表示言語

「設定」→「一般」の「言語」で「日本語」と「English」を切り替えられます。

---

## 2. タブを管理する

### 2.1 タブを作る・登録する

タブを追加する方法は3つあります。

**方法 A: 新しい空のタブを作る**

1. タブバーの「＋」(ツールチップ「新しいタブを作成」)をクリックします。
2. 「新しいタブを作成」ダイアログの「タブ（フォルダ）名」に名前を入力し、「作成」を押します。
3. `data/tabs/` の下にその名前のフォルダが作られ、タブとして登録されます。同じ名前のフォルダがすでにある場合は、`名前(1)`、`名前(2)` のように番号がつきます。

**方法 B: すでにあるフォルダを登録する**

1. タブバーの「📂」(ツールチップ「既存フォルダを登録」)をクリックします。
2. Windows のフォルダ選択画面で、登録したいフォルダを選びます。
3. そのフォルダがタブになります。フォルダの中に、このアプリが管理用に作る隠しファイル `.tabtreenote.json` があれば、その内容(階層やラベルなど)が引き継がれます。

**方法 C: フォルダをドラッグして登録する**

エクスプローラーからフォルダをタブバーの上にドラッグ&ドロップします(方法 B と同じ登録になります)。

タブが1つも無いときは、画面に「登録されているタブがありません」と表示され、「既存フォルダを登録」と「新しいタブを作成」のボタンが出ます。

**名前の重複について**: タブの名前は重複できません。登録しようとしたフォルダ名がすでに使われている場合は、「タブ名の重複」ダイアログが開くので、「別の名前を入力してください」に従って別の名前を入力します。

**使えない名前**: 名前に `\ / : * ? " < > |` は使えません。Windows で予約されている名前、先頭や末尾の空白、末尾のピリオドも使えません。入力欄の下にその理由が表示されます。

### 2.2 タブを切り替える

- タブをクリックします。
- Ctrl+Tab で次のタブ、Ctrl+Shift+Tab で前のタブに切り替わります。

アクティブな(選択中の)タブをダブルクリックすると、開いているメモの先頭(1行目)にカーソルとスクロールが移動します(タブにマウスを重ねると「(ダブルクリックで先頭へ)」と表示されます)。もう一度ダブルクリックすると、移動前の位置に戻ります。タブやメモを切り替えると、戻り先の記憶は消えます。

### 2.3 タブの並べ替え

タブをマウスでドラッグして、好きな位置に動かします。並び順は保存されます。

### 2.4 タブの右クリックメニュー

タブを右クリックすると、次のメニューが開きます。

| メニュー | できること |
|---|---|
| 「名前を変更」 | タブの表示名を変更します(ロック中のタブでは選べません)。フォルダ自体の名前は変わりません。 |
| 「ロック済みタブのラベルを設定」 | ロック中のタブを見分けるための自分用ラベルを設定します(7.6 参照)。空欄にすると解除されます。 |
| 「タブの場所を開く」 | そのタブのフォルダをエクスプローラーで開きます。 |
| 「タブをロック」 / 「ロック解除」 | パスワードでタブを保護します(7 参照)。 |
| 「完全にロック解除」 | ロック済みのタブにだけ表示されます(7.5 参照)。 |
| 「タブを削除」 | タブを一覧から外します(2.5 参照)。 |

### 2.5 タブを削除する

1. タブを右クリックして「タブを削除」を選びます。
2. 「「(タブ名)」をタブ一覧から削除します。フォルダやファイルは削除されません。」と表示されるので、「削除」を押します。

> 重要: 「タブを削除」は**登録を外すだけ**です。フォルダの中のメモや画像は消えません。あとで「既存フォルダを登録」から再登録できます。

ロック中のタブ(ロック解除前)は削除できません。先にロックを解除してください。

### 2.6 そのほかのタブバーのボタン

- **★**(「お気に入り」): お気に入りに登録したメモの一覧を開きます(3.8 参照)。
- **📌**(「最前面に固定」): アプリのウィンドウをほかのウィンドウより前面に固定します。もう一度押すと「最前面固定を解除」になります。この設定は保存されず、次の起動では元に戻ります。
- **🔍**: 全タブ横断の「全文検索」を開きます(5.2 参照)。
- **⇄**(「ツリーの位置を切り替え」): ツリーを画面の左側と右側で入れ替えます。
- **⚙**(「設定」): 設定画面を開きます(8 参照)。

---

## 3. メモを管理する

### 3.1 メモを作る

- ツリーが空のときは、中央の「＋ メモを作成」ボタンを押します。
- ツリーの何も無い場所を右クリックして「メモを作成」を選びます(一覧の末尾に追加されます)。
- メモを右クリックして「メモを作成」(末尾に追加)または「子として作成」(そのメモの子として追加)を選びます。

どの場合も、名前の入力ダイアログ(「メモ名」)が開きます。名前を入力して「作成」を押すと、その名前の `.md` ファイルが作られます。同じ名前のメモがすでにある場合は、`名前(1)` のように番号がつきます。

メモ名に使えない文字は、タブ名と同じです(2.1 参照)。

### 3.2 メモを開く・移動する

- メモをクリックすると、右側のエディタで開きます。
- ↑ / ↓ キーで前後のメモに移動できます(ツリーにフォーカスがあるとき)。
- 親メモの左側の開閉マークで、子メモを展開・折りたたみできます。

### 3.3 複数選択

Ctrl+クリックで1つずつ追加選択、Shift+クリックで範囲選択できます。何も無い場所をクリックすると選択が解除されます。複数選択したメモは、まとめて削除したり、まとめてドラッグしたりできます。

### 3.4 名前を変更する

- メモを右クリックして「名前を変更」を選ぶか、メモを選んで **F2** キーを押します。
- 名前を入力して Enter で確定します。

メモ名の変更は、そのメモのファイル名の変更です。画像を貼り付けたメモでは、画像用のフォルダ(4.6 参照)も同じ名前に変更されます。

> 注意: メモ名を変えると、そのメモを指す `memo:` リンク(6.4 参照)は無効になります。リンクは自動では書き換わらないので、必要なら手で直してください。

### 3.5 並べ替え・階層の変更・移動(ドラッグ&ドロップ)

メモをドラッグして、ほかのメモの上にドロップします。

- メモの行の**上のほう**にドロップすると、そのメモの直前に入ります。
- メモの行の**下のほう**にドロップすると、そのメモの直後に入ります。
- メモの行の**真ん中あたり**にドロップすると、そのメモの子になります。

**ほかのタブへ移動・コピー**: メモをドラッグして、タブバーのほかのタブの上にドロップします。そのまま移動になります。**Ctrl キーを押しながら**ドロップすると、移動ではなくコピーになります。

ほかの通常のタブへ移動・コピーすると、画像用のフォルダ(4.6 参照)も、メモといっしょに移ります。

ロック中のタブにドロップすると、パスワード入力のダイアログ(「ロックされたタブにドロップ」)が開きます(7.7 参照)。

### 3.6 メモを削除する(ごみ箱)

1. メモを右クリックして「削除」を選びます(子メモもいっしょに削除されます)。
2. 「メモを削除」ダイアログに「選択した ○ 件のメモ（子メモを含む）をごみ箱に移動します。」と表示されるので、「はい」を押します。

削除したメモは、Windows の**ごみ箱**に移動します。ごみ箱から元に戻せます。

- ごみ箱に移動できなかったときは、「ごみ箱への移動に失敗しました」ダイアログで「完全に削除」を選ぶかどうか確認されます(完全に削除すると取り消せません)。
- **Ctrl+Delete**(設定上の名前は「強制削除」)でも、選択中のメモを削除できます。エディタにフォーカスがあっても使えます。同じ確認ダイアログが開きます。
- そのメモ用の画像フォルダ(4.6 参照)があれば、メモといっしょにごみ箱へ移動されます。画像フォルダだけ削除できなかったときは、「メモは削除しましたが、画像フォルダを削除できませんでした」と通知されます。

### 3.7 編集ロック(メモ単位の保護)

誤って書き換えないよう、メモ1つ1つを編集禁止にできます。パスワードは不要です(パスワードで守る「タブロック」とは別の機能です)。

- 設定: メモを右クリックして「編集ロック」を選びます。解除は「編集ロック解除」です。
- 編集ロック中のメモは、ツリー上で見た目が変わり(メモ名にマウスを重ねると「編集ロック中」と表示されます)、エディタのステータスバーに「🔒 編集ロック中」と出ます。
- 本文を編集できません。ツールバーも並べ替えボタン以外は押せなくなります。
- **名前の変更と削除もできません**(「編集ロック中のメモは削除・名前変更できません」)。子メモに編集ロック中のメモがある場合、親メモの削除もできません。
- 編集ロックをかけたり外したりすると、開いているメモにもすぐ反映されます。

### 3.8 お気に入り

- メモを右クリックして「お気に入りに追加」、外すときは「お気に入りから削除」を選びます。
- タブバーの「★」を押すと、「お気に入り」の一覧が開きます。メモをクリックするか、↑ / ↓ と Enter で、そのメモをタブごと開けます。一覧の中の星マークをクリックすると、お気に入りから外れます。
- キーボードで一覧を開きたいときは、「設定」→「ショートカット」の「お気に入り一覧を開く」にキーを割り当てます(初期状態では「未設定」です)。
- ロック中のタブのメモは、お気に入り一覧に出ません。

### 3.9 アイコン(色ラベル)

メモを右クリックして「アイコン」を開くと、「なし」「🔴 赤」「🔵 青」「🟢 緑」「🟡 黄」から選べます。メモを見分けるための色の目印です。

### 3.10 プロパティと保存場所

- 「プロパティ」: ファイル名、ファイルパス、作成日時、更新日時、ファイルサイズに加え、総字数、字数(改行・空白を除く)、行数が表示されます。エディタの右クリックメニューからも開けます。
- 「ファイルの場所を開く」: そのメモのファイルをエクスプローラーで開きます。

### 3.11 エクスプローラーから `.md` ファイルを取り込む

エクスプローラーから `.md` ファイルをツリーの上へドラッグ&ドロップすると、メモとして取り込めます。

- **コピーです**。元のファイルは変更も削除もされません。
- メモの行の上にドロップすると、そのメモの子(子の並びの末尾)として取り込まれます。ツリーの何も無い場所にドロップすると、一番上の階層の末尾に取り込まれます。メモ名は、ファイル名から `.md` を除いたものです。
- `.md` 以外のファイルとフォルダは取り込まれません(読み飛ばされます)。スキップした項目があったときは、取り込みのあと、「○件をインポートしました（○件をスキップ）」と通知されます。
- 同じ名前のメモがすでにある場合は、「インポート時の名前の重複」ダイアログで、「スキップ」「上書き」「名前を変更してインポート」から選びます。「残りの重複項目すべてにこの選択を適用する」にチェックを入れると、同じ選択をまとめて適用できます。重複ダイアログで Esc を押すと、その項目とまだ処理していない残りの項目は取り込まれず(スキップ扱い)、取り込みが中止されます。ただし、Esc より前に処理が決まっていた項目(重複のなかったものや、すでに選択済みのもの)は取り込まれます。
- 編集ロック中のメモには「上書き」できません。

---

## 4. メモを編集する

### 4.1 表示モード

**ライブ表示とソース表示**

- **ライブ表示**(初期設定): Markdown の記号(`#` や `**` など)を、見出しや太字などの見た目に変えて表示しながら編集します。カーソルがある部分は、元の記号が見えます。
- **ソース表示**: Markdown の記号がそのまま見える、素のテキストとして編集します。

ツールバーの上部にあるボタンで切り替えます。ボタンには、**押すと切り替わる先の名前**が表示されます(ライブ表示のときは「ソース表示」、ソース表示のときは「ライブ表示」)。ショートカットは Ctrl+Shift+M です。表示を切り替えても、メモの文章自体は変わりません。

**ペイン表示(編集のみ・分割表示・プレビューのみ)**

ツールバー上部の、ペイン表示のボタンを押すたびに、次の順で切り替わります。

1. 「編集のみ」: エディタだけを表示します。
2. 「分割表示」: エディタとプレビュー(完成形の表示)を並べて表示します。
3. 「プレビューのみ」: プレビューだけを表示します。

ボタンには、ライブ/ソースの切り替えと同じく、**押すと切り替わる先の名前**が表示されます。ショートカットは Ctrl+Shift+P です。

- 「プレビューに固定」(鍵のアイコン)を押すと、そのメモをプレビュー表示のまま固定し、ペイン表示のボタンを押せなくします。「プレビュー固定を解除」で元に戻ります。
- 「プレビューのみ」では、ツールバーが自動的に最小化され、「－」「＋」とパーセント表示のズーム操作が現れます(4.9 参照)。
- 表示モードとペイン表示は、メモごとに記憶されます。新しいメモの初期値は「設定」で変えられます(8.3 参照)。

**分割表示の向き**: 「設定」→「エディタ」の「分割表示の向き」で、「左右に並べる」か「上下に並べる」を選べます。

### 4.2 基本の編集

- 元に戻す(Ctrl+Z)・やり直す(Ctrl+Y)、切り取り・コピー・貼り付け、すべて選択は、エディタの右クリックメニューからも使えます。
- 折り返し、タブ幅、行番号、フォントサイズは「設定」の「エディタ」で変えられます(8.3 参照)。
- Tab キーの動作(スペースを入れるか、タブ文字を入れるか)は、「タブをスペースとして挿入する」で選びます。
- メモごとのスクロール位置が記憶されます。

**エディタの右クリックメニュー**

| メニュー | 内容 |
|---|---|
| 「リンクをコピー」 | リンクの上で右クリックしたときだけ表示されます(6.6 参照)。 |
| 「元に戻す」「やり直す」「切り取り」「コピー」「貼り付け」「すべて選択」 | 通常の編集操作です。 |
| 「行操作」 | 「行を上に移動」「行を下に移動」「行を複製」「行を削除」 |
| 「変換」 | 「大文字に変換」「小文字に変換」「ひらがなに変換」「カタカナに変換」「半角に変換」「全角に変換」「タブをスペースに」「スペースをタブに」 |
| 「チェック（☐/☑）」 | カーソルのある行の ☐ と ☑ を切り替えます。行に無ければ、カーソル位置に ☐ を入れます。 |
| 「Googleで検索」 | 選択した文字列を、既定のブラウザで Google 検索します。 |
| 「検索と置換」 | メモ内の検索と置換を開きます(5.1 参照)。 |
| 「空白/改行を表示」 | 空白と改行の記号の表示を切り替えます(「空白/改行の表示を消す」で非表示)。 |
| 「プロパティ」 | ファイル情報と文字数を表示します。 |

編集ロック中のメモでは、内容を変更するメニューが選べなくなります。

### 4.3 ツールバー

エディタの上に、書式を付けるためのボタンが並んだ「ツールバー」が浮かんでいます(9列×2行)。各ボタンは、マウスを重ねると名前が表示されます。選択した文字に対して、または、カーソル位置に対して働きます。

| ボタン名 | 働き |
|---|---|
| 「元に戻す」「やり直す」 | 直前の操作を取り消す/やり直す |
| 「文字色」 | 色の一覧(「赤」「青」「緑」「橙」「紫」)を開きます。選んだ色を選択範囲に付けます。「解除」で色を外します。 |
| 「文字サイズ」 | 「小」「大」「特大」の一覧を開きます。「解除」でサイズを元に戻します。 |
| 「見出し」 | カーソル行の見出しレベルを、見出し無し → 1 → 2 → … → 6 → 見出し無し、の順に切り替えます。 |
| 「リスト」 | 選択した行の先頭に「- 」(箇条書き)を付けたり外したりします。 |
| 「太字」「斜体」「取り消し線」 | 選択範囲に付けたり外したりします。 |
| 「引用」 | 選択した行の先頭に「> 」を付けたり外したりします。 |
| 「リンク挿入」 | `[リンク](…)` の形でリンクを挿入します。 |
| 「コードブロック」 | 選択範囲をコードブロックで囲みます。選択が無ければ空のコードブロックを入れます。 |
| 「目次を更新」 | 目次を作ります(4.8 参照)。 |
| 「表挿入」 | 2列の表のひな形を、カーソル位置に入れます。 |
| 「画像挿入」 | 画像ファイルを選んで挿入します(4.6 参照)。 |
| 「書式クリア」 | 選択範囲の書式を取り除きます。 |
| 「強制改行」 | 行末にスペースを2つ入れて改行します(Markdown の強制改行)。 |
| 「並べ替えモード」 | ボタンをドラッグして並べ替えられるモードに入ります。Esc または、もう一度このボタンで終了します。並び順は保存されます。 |

ツールバー上部の帯(「⋮⋮」のある部分)には、次の操作もあります。

- 帯をドラッグすると、ツールバーの位置を動かせます。位置は保存されます。
- 「ツールバーを最小化」/「ツールバーを元に戻す」で、ボタンの一群を畳んだり開いたりできます。
- 「PDF出力」(Ctrl+P): プレビューと同じ見た目の PDF を作ります。保存先を選ぶ画面が開きます。メモを開いていないときは押せません。
- ライブ表示/ソース表示の切り替えとペイン表示の切り替え(4.1 参照)。

ここに出てくる主なボタンには、ショートカットキーがあります(9 参照)。

### 4.4 Markdown の書き方(基本)

CommonMark に準拠した Markdown を使えます。見出し、リスト、引用、コードブロック、表、太字・斜体、水平線、リンクなどが使えます。

| 書き方 | 結果 |
|---|---|
| `# 見出し` | 見出し(`#` の数でレベルが決まります。6つまで) |
| `**太字**` / `*斜体*` / `~~取り消し線~~` | 太字 / 斜体 / 取り消し線 |
| `- 項目` | 箇条書き |
| `> 引用` | 引用 |
| `[文字](リンク先)` | リンク |
| `![説明](画像のパス)` | 画像 |

文字色と文字サイズには Markdown の標準の書き方がないため、`<span style="color:…;font-size:…">` という形で保存されます(ツールバーから設定すると自動で入ります)。

> 安全のため、プレビューでは、許可された書き方(`<img>` と、色・サイズだけの `<span style>`)以外の HTML は取り除かれます。`<script>` などは動きません。

### 4.5 チェックボックス

- **クリックで切り替え**: ライブ表示では、`- [ ]` と `- [x]` の形のチェックボックスをクリックするとチェックが入る/外れます(マウスを重ねると「クリックでチェック」/「クリックでチェックを外す」と出ます)。編集ロック中は切り替わりません。
- **右クリックで切り替え**: カーソルのある行で、右クリックメニューの「チェック（☐/☑）」を選ぶと、☐ と ☑ が入れ替わります。

### 4.6 画像を貼り付ける・挿入する

次のどれでも、画像をメモに入れられます。

- クリップボードの画像を、エディタに貼り付ける(Ctrl+V または右クリックの「貼り付け」)。
- ツールバーの「画像挿入」で、画像ファイルを選ぶ。
- エクスプローラーから画像ファイルをエディタにドラッグ&ドロップする。

対応する画像の種類は、png、jpg、jpeg、gif、webp、bmp です。

- 通常のタブでは、画像は **`タブのフォルダ/メモ名/`** という画像用フォルダに、`001.png`、`002.png` … の連番で保存されます。メモ名を変えると、このフォルダの名前も変わります。
- ロック中のタブのロック解除中は、画像はタブのフォルダ直下に、ランダムな名前(`img_…`)で保存されます(メモ名を外から見えなくするためです)。
- 大きな画像は、保存前に自動で縮小・圧縮されることがあります(長辺が 2000 ピクセルを超える画像は縮小、1MB を超える画像は WebP に変換して軽くします。GIF は変換されません)。
- メモ本文から画像の記述を消しても、画像ファイルは**自動では削除されません**。不要な画像は、エクスプローラーで手動で整理してください(メモ自体を削除したときの扱いは 3.6 を参照)。
- プレビューで画像をクリックすると、「画像」ダイアログで拡大表示されます。

### 4.7 数式

TeX の書き方で数式を書けます。

- 文中の数式: `$数式$`
- 独立した数式: `$$数式$$`

数式が実際の数式の形で表示されるのは**プレビュー**(「分割表示」「プレビューのみ」)です。ライブ表示とソース表示では、数式の部分に目印がつくだけで、数式の形では表示されません。

> 注意: 「$」を金額などの普通の文字として使っていると、数式と見なされる場合があります。

### 4.8 目次

1. 見出し(`#` から `######`)を使ってメモを書きます。
2. 目次を入れたい場所にカーソルを置き、ツールバーの「目次を更新」(Ctrl+Shift+O)を押します。
3. 見出しへのリンクが入れ子の箇条書きで挿入されます。`<!-- TOC start -->` と `<!-- TOC end -->` の間の部分が目次です。

目次は**ボタンを押したときだけ**更新されます。見出しを変えたら、もう一度「目次を更新」を押してください。すでに目次がある場合は、その中身が入れ替わります。目次のリンクをクリックすると、そのメモの中の該当する見出しへ移動します。

### 4.9 プレビューのズームと PDF 出力

- 「プレビューのみ」のときだけ、ツールバー上部に「－」「＋」と現在の倍率(%)が表示されます。70% から 180% まで、10% ずつ変えられます。倍率の表示をクリックすると、100% に戻ります。
- キーボードでは Ctrl+= で拡大、Ctrl+- で縮小です(「設定」で変更できます。テンキーの Ctrl+「+」「-」でも操作できます)。
- 拡大率は保存されません。タブやメモを切り替えたり、「プレビューのみ」から出たりすると、100% に戻ります。
- 分割表示のプレビューは、常に 100% の大きさです。

---

## 5. 検索と置換

### 5.1 メモの中を検索・置換する

1. メモを開いて Ctrl+F(または右クリックの「検索と置換」)を押します。
2. 上の入力欄に探す文字を入力します。入力に合わせて一致する箇所が探されます。「▼」(「次へ」)、「▲」(「前へ」)、または Enter / Shift+Enter で、次/前の候補へ移動します。「すべて選択」を押すと、一致する箇所をすべて選択します。
3. 置換するときは、下の入力欄(「置換後の文字列」)に入力し、「置換」で1件ずつ、「すべて置換」でまとめて置き換えます。置換欄で Enter を押しても、1件置換されます。
4. オプションで「大文字/小文字を区別」「単語単位」「正規表現」を選べます。
5. 「×」(「閉じる (Esc)」)または Esc で、検索パネルを閉じます。

過去に検索した語は、入力欄の候補として表示されます。

### 5.2 全タブを横断して検索する(全文検索)

1. タブバーの「🔍」を押します。「全文検索」ダイアログが開きます。
2. 「検索語を入力...」に探す語を入力し、「検索」(または Enter)を押します。
3. 範囲は「全タブ」と「現在のタブ」から選べます(初期値は「現在のタブ」です)。「大文字小文字区別」と「正規表現」も選べます。
4. 結果の一覧に、メモ名・タブ名・一致した場所(「名前」「本文」「名前・本文」)が出ます。クリックすると、そのタブのそのメモが開きます。

- 検索の対象は、メモの名前と本文です。
- **ロック中のタブ(ロックを解除して一時的に開いている間も)は、検索の対象になりません。**
- 結果が無いときは「一致する結果はありません」と出ます。

### 5.3 クイックオープン(メモ名ですばやく開く)

1. **Ctrl+O** を押します。「クイックオープン」ダイアログが開きます。
2. 「メモ名で検索...」に、メモ名の一部を入力します。全タブのメモが候補に出ます。候補には、メモ名とタブ名が並びます。
3. ↑ / ↓ で候補を選び、Enter で開きます。クリックでも開けます。

候補の右にある星(☆/★)を押すと、そのメモをお気に入りに追加・削除できます。ロック中のタブのメモは、候補に出ません。ショートカットのキーは「設定」で変更できます。

---

## 6. リンク

### 6.1 リンクの種類

エディタ(ライブ表示)とプレビューで、次のリンクをクリックして開けます。

| 種類 | 書き方の例 | 開き先 |
|---|---|---|
| URL | `https://example.com` または `www.example.com` | 既定のブラウザ |
| メール | `name@example.com` | 既定のメールソフト |
| ファイルパス | `C:\Users\…\memo.txt`、`./相対パス/ファイル` | そのファイルを関連付けられたアプリで開く |
| FTP | `[表示する文字](ftp://example.com/file)` | 既定のアプリ(OS が `ftp://` に関連付けているもの) |
| メモ間リンク | `memo:メモ名` など(6.4 参照) | アプリ内でそのメモを開く |
| 見出しリンク | `[見出し](#見出し名)`(目次が作ります) | 同じメモの中の見出しへ移動 |

- `[表示する文字](リンク先)` の形でも、リンクを書けます。
- `ftp://` のリンクは、`[表示する文字](ftp://…)` の形で書くと、ライブ表示でもプレビューでも開けます(OS の既定の関連付けに渡されます)。`ftp://…` だけを書いた場合は、プレビューでは自動でリンクになりますが、ライブ表示では自動ではリンクになりません。
- 日本語を含む URL やメールアドレスにも対応しています。URL の途中に日本語の句読点(「。」「、」など)が来ると、そこで URL が終わったものとして扱われます。
- 相対パスのファイルパス(`./` や `../` で始まるもの)は、そのメモのタブのフォルダを基準に探されます。
- 見つからないファイルや、解決できない `memo:` リンクは、何も起きません。

### 6.2 リンクを開く操作(開き方の設定)

「設定」→「一般」の「リンク」で設定します。

- 「開くトリガー」: 「シングルクリック」(初期値)、「Ctrl+クリック」、「ダブルクリック」から選びます。
- 「開く前に確認する」: オンにすると、リンクを開く前に「外部リンクを開く」ダイアログで確認されます。

### 6.3 実行ファイルを開くときは必ず確認されます

ファイルパスのリンクが、実行ファイルやスクリプト(`.exe`、`.bat`、`.ps1` など)を指している場合は、「開く前に確認する」がオフでも、**必ず**「実行ファイルを開く」ダイアログで確認されます。「実行ファイルまたはスクリプトを開こうとしています。信頼できるファイルでない場合は実行しないでください。」と表示されるので、心当たりのあるファイルだけを開いてください。

### 6.4 メモ間リンク(`memo:`)

メモからほかのメモへリンクできます。

| 書き方 | リンク先 |
|---|---|
| `memo:メモ名` | 同じタブの、そのメモ |
| `memo:タブ名/メモ名` | 指定したタブの、そのメモ |
| `memo:タブ名/` | 指定したタブ |
| `[表示する文字](memo:タブ名/メモ名)` | 表示する文字つきのリンク |
| `memo:タブ名/メモ名#見出し名` | そのメモの、指定した見出し(`#` 以降は目次のリンクと同じ見出し名の書き方) |

- ロック中のタブ(ロックを解除していない間)のメモは、リンクが無効の扱いになります。ロック解除中は通常どおり開けます。
- メモ名を変更すると、リンクは無効になります(3.4 参照)。

### 6.5 見出しリンク

目次(4.8)が作る `[見出し](#見出し名)` のリンクは、そのメモの中の該当する見出しへ移動します。一致する見出しが無いときは、何も起きません。

### 6.6 リンクをコピーする

URL・メール・ファイルパスのリンクの上で右クリックすると、メニューの先頭に「リンクをコピー」が出ます。リンク先をクリップボードにコピーします(エディタとプレビューのどちらでも使えます)。

---

## 7. タブロック

### 7.1 タブロックとは

パスワードで、**タブの中身をまとめて**保護する機能です。ロックされたタブは、メモの一覧も、メモの内容も、タブの名前(代わりに 🔒 と表示)も隠れます。

- メモ1件ごとのパスワード保護はありません。保護できるのは、タブ全体だけです。
- 暗号化の方式は AES-256-GCM(パスワードからの鍵づくりに scrypt)です。
- ロック中のタブは、全文検索・クイックオープン・お気に入り一覧の対象になりません。ロック中のタブのメモへの `memo:` リンクも開きません。
- ロック中のタブは削除できません(先にロックを解除します)。

> 注意: ロックすると、タブのフォルダの中の `.md` ファイルは、**暗号化され、ファイル名もランダムな文字列に変わります**。フォルダをエクスプローラーで開いても、元のメモ名や内容は読めません。ロックを使うときは、7.8 の注意点もお読みください。

### 7.2 タブをロックする

1. タブを右クリックして「タブをロック」を選びます。
2. 「「(タブ名)」をロック」ダイアログで、「パスワード」と「パスワード（確認）」を入力します。「ヒント（任意）」には、パスワードを思い出すための手がかりを入れられます。
3. 「ロック」を押します。

- 2つのパスワードが違うときは、「パスワードが一致しません」と出ます。
- そのタブで開いていたメモは、保存されてから閉じられます。
- ロックが終わると、タブの表示が 🔒 に変わります。ツリーには「🔒 このタブはロックされています」と「🔓 ロック解除」ボタンが出ます。

### 7.3 ロックを解除する(一時的に開く)

1. 「🔓 ロック解除」ボタンを押すか、タブを右クリックして「ロック解除」を選びます。
2. パスワードを入力して「ロック解除」を押します。ヒントを設定していた場合は、入力画面に「ヒント: …」と表示されます。
3. パスワードが違うと、「パスワードが正しくありません」と出ます。

解除に成功すると、本当のタブ名とメモの一覧が見え、普通のタブと同じように編集できます。このとき、ツリーの上に「🔓 このタブは一時的にロック解除されています」と表示されます。

この状態はあくまで**一時的**で、次の場合に、再び自動でロックされます。

- ほかのタブへ切り替えたとき。
- アプリのウィンドウがほかのアプリに切り替わって、フォーカスを失ったとき。
- 一定時間、キーボードやマウスの操作が無かったとき(7.4 参照)。

再び開くには、そのたびにパスワードの入力が必要です。ロックされるときは、編集中の内容が先に保存されます。

### 7.4 一定時間操作が無いと自動でロックする

「設定」→「一般」の「タブロック」→「非アクティブによる自動ロック」で、「5分」「15分」「30分」「60分」「無効」から選びます(初期値は 15 分です)。

- ロックされる 30 秒ほど前に、「まもなく非アクティブのためロックされます」と通知されます。ロックされたあとは、「非アクティブのため自動的にロックされました」と通知されます。
- ダイアログが開いている間は、時間の数え上げが止まります。
- 解除中のタブにメモの画像を挿入するため、ツールバーの「画像挿入」のファイル選択画面を開いている間(最大2分)は、フォーカスを失っても、すぐにはロックされません。

### 7.5 完全にロックを解除する(元の普通のタブに戻す)

パスワード保護そのものをやめて、元の普通のタブに戻す操作です。

- ロック中のタブを右クリックして「完全にロック解除」を選ぶ。
- または、ロックを一時的に解除している間に、ツリー上部の「完全にロック解除」ボタンを押す。

確認ダイアログに「完全にロック解除すると、暗号化前の通常のタブに戻ります。この操作は取り消せません。」と表示されます。「念のため、事前にフォルダ全体のバックアップを取っておくことをおすすめします。」とあるとおり、先にフォルダをコピーしておくと安心です。実行すると、すべてのメモが復号され、ファイル名も元のメモ名に戻ります。ロック中のタブから実行する場合は、パスワードの入力が必要です。

> 完全に解除したあとにもう一度ロックしても、解除していた間は暗号化されていなかったという事実は取り消せません。

### 7.6 ロック中のタブを見分けるラベル

ロック中のタブは、名前が隠れて、どれも 🔒 だけの表示になります。タブを右クリックして「ロック済みタブのラベルを設定」を選ぶと、自分で決めたラベルを付けられます(「ラベル（空欄で解除）」)。ロック中のタブは「🔒 ラベル」と表示されます。

> 注意: ラベルは**暗号化されず、そのままの文字で**アプリの設定ファイル(`data/settings.json`)に保存されます。ロック中でも読めますし、設定ファイルを見た人にも分かります。ラベルは、ロックされたタブを見分けるためのものです。本当のタブ名やメモの内容に関わる、人に知られたくない言葉は入れないでください。

### 7.7 ほかのタブとのメモのやりとり

- ほかのタブから、ロック中のタブへメモをドラッグしてドロップすると、「ロックされたタブにドロップ」ダイアログが開きます。パスワードを入力して「ロック解除して移動」を押すと、そのメモが移動(Ctrl を押していればコピー)されます。パスワードを入力せずに閉じると、何も起きません。
- ロックを解除して開いているタブから、ほかの普通のタブへメモを移すこともできます。移したメモは、**暗号化されていない**普通の `.md` ファイルになります。

### 7.8 注意点

- **パスワードを忘れたときの復元機能は、このアプリにはありません。** 内容を読むにはパスワードが必要な仕組みで、パスワードの再発行や、運営側で解除する手段は用意されていません。パスワードは忘れないように管理してください。
- 「ヒント」は、**暗号化されず、そのままの文字で**、タブのフォルダの中の `.tabtreenote.json` に保存されます(パスワードを入力する前の画面に表示するためです)。フォルダを見た人にも読めます。パスワードそのものや、すぐにパスワードが分かる言葉は入れないでください。
- ロックを使う前や、完全にロック解除する前には、フォルダ全体のコピーでバックアップを取っておくと安心です(フォルダのコピーは、ロック中でもできます)。
- ロックの途中でアプリが止まるなどして処理が中断した場合、そのタブを開くと「前回のロック操作が完了しませんでした。復旧処理が必要です。」と表示されます。「ロック処理の復旧」ダイアログでパスワードを入力し、「ロックを完了する」か「元に戻す（ロックを取り消す）」を選びます。完全にロック解除する処理が中断した場合も同様に、「完全ロック解除の復旧」の画面で続きから進めるか、ロックを元に戻すかを選べます。「想定外の状態のファイルが見つかりました」と出たときは、手動での確認が必要です。
- ロック中のタブの名前は、ロックを解除するまで変更できません。

---

## 8. 設定

タブバーの「⚙」(「設定」)を押すと、「設定」画面が開きます。上部の「一般」「ショートカット」「エディタ」のタブで画面を切り替えます。変更はすぐに反映され、「適用」のような操作は要りません。「閉じる」で閉じます。

### 8.1 一般

**外観**

- 「テーマ」: 「ライト」「ダーク」
- 「UI文字サイズ (○%)」: 画面の文字(エディタ本文以外)の大きさを 80% から 150% で変えます。
- 「アクセントカラー」: 強調の色を選びます。「↺」(「既定に戻す」)で初期の色に戻します。
- 「UIフォント」: 画面の文字のフォント名を入力します。「↺」で初期のフォントに戻します。
- 「ツリーの行間 (○)」: ツリーの行の高さを変えます。
- 「言語」: 「日本語」「English」

**リンク**(6.2 参照)

- 「開くトリガー」: 「シングルクリック」「Ctrl+クリック」「ダブルクリック」
- 「開く前に確認する」(実行ファイル・スクリプトへのリンクは、オフでも必ず確認されます)

**スタートアップ**

- 「Windowsログイン時にトレイへ自動起動する」: オンにすると、Windows にログインしたとき、アプリがタスクトレイに入った状態で起動します。トレイのアイコンをクリックすると、ウィンドウが表示されます。なお、ウィンドウの「閉じる」ボタン(×)は、この設定に関係なくアプリを終了します(トレイには入りません)。

**タブロック**(7.4 参照)

- 「非アクティブによる自動ロック」: 「5分」「15分」「30分」「60分」「無効」

**サードパーティ ライセンス**

画面の一番下の「サードパーティ ライセンス」を押すと、このアプリが利用しているソフトウェアのライセンス表示が開きます。

### 8.2 ショートカット

操作ごとのショートカットキーの一覧です(既定のキーは 9 を参照)。キーを変更できます(「次のタブ」「前のタブ」を除く。この2つは現在は既定のキーのみ有効です。9 参照)。

1. 変更したい操作の行にあるキーのボタンを押します。「キー入力待ち...」と表示されます。
2. 割り当てたいキーの組み合わせを押します(Ctrl や Shift だけでは確定しません。通常のキーまで押してください)。
3. やめるときは Esc を押します。
4. 変更した行には「↺」(「既定に戻す」)が出ます。押すと初期のキーに戻ります。

キーが割り当てられていない操作は、「未設定」と表示されます。

### 8.3 エディタ

**エディタの既定値**

- 「折り返し」: 「ウィンドウ幅で折り返す」「文字数で折り返す」「折り返さない」
- 「タブ幅」(1 〜 16)
- 「タブをスペースとして挿入する」
- 「行番号を表示する」
- 「フォントサイズ」(8 〜 40)
- 「新規メモの既定のペイン表示」: 「編集のみ」「分割表示」「プレビューのみ」
- 「分割表示の向き」: 「左右に並べる」「上下に並べる」

**プレビュー**

- 「プレビュー再描画の遅延 (ミリ秒)」: 入力してからプレビューが更新されるまでの待ち時間です(50 〜 2000)。

**自動保存**

- 「自動保存の遅延 (ミリ秒)」: 入力が止まってから自動で保存されるまでの待ち時間です(200 〜 10000)。

**改行コード**

- 「保存時のポリシー」: 「読み込み時の形式を維持」(初期値)、「CRLF」、「LF」、「CR」
- 「文字コード」: 「UTF-8（固定）」(変更できません)

---

## 9. ショートカット一覧

下の表は、初期状態のキーです。「設定」→「ショートカット」で変更できます(8.2 参照)。ただし「次のタブ」「前のタブ」は、設定画面に表示されますが、現在は変更が反映されず、Ctrl+Tab / Ctrl+Shift+Tab のままです。

**タブ・ツリー・検索**

| 操作 | 既定のキー |
|---|---|
| 次のタブ | Ctrl+Tab |
| 前のタブ | Ctrl+Shift+Tab |
| 保存 | Ctrl+S |
| 検索/置換を開く | Ctrl+F |
| 名前を変更(ツリーで選択したメモ) | F2 |
| 強制削除(ツリーで選択したメモを削除) | Ctrl+Delete |
| クイックオープン | Ctrl+O |
| お気に入り一覧を開く | 未設定(自分で割り当て) |

**書式・表示**

| 操作 | 既定のキー |
|---|---|
| 元に戻す | Ctrl+Z |
| やり直し | Ctrl+Y |
| 文字色 | Ctrl+Shift+C |
| 文字サイズ | Ctrl+Shift+F |
| 見出し | Ctrl+Shift+H |
| リスト | Ctrl+Shift+L |
| 太字 | Ctrl+B |
| 斜体 | Ctrl+I |
| 取り消し線 | Ctrl+Shift+X |
| 引用 | Ctrl+Shift+Q |
| リンク挿入 | Ctrl+K |
| コードブロック | Ctrl+Shift+K |
| 目次を更新 | Ctrl+Shift+O |
| 表を挿入 | Ctrl+Shift+T |
| 画像を挿入 | Ctrl+Shift+I |
| 書式クリア | Ctrl+Shift+E |
| 強制改行 | Shift+Enter |
| ツールバー並べ替え | Ctrl+Shift+R |
| ライブ/ソース表示切替 | Ctrl+Shift+M |
| ペイン表示モードを切り替え | Ctrl+Shift+P |
| PDF出力 | Ctrl+P |
| プレビューをズームイン | Ctrl+= |
| プレビューをズームアウト | Ctrl+- |

**そのほか(変更できないキー)**

| 操作 | キー |
|---|---|
| ツリーで前後のメモへ移動 | ↑ / ↓ |
| 検索パネルで次/前の候補 | Enter / Shift+Enter |
| 検索パネル・ダイアログを閉じる | Esc |
| プレビューのズーム(テンキー) | Ctrl+テンキーの「+」/「-」 |

---

## 10. 困ったとき

### 10.1 ほかのプログラムでメモが変更されたとき(競合の確認)

メモを開いている間に、ほかのプログラムがそのファイルを書き換えると、「外部でファイルが変更されています」というダイアログが開きます。

- 「このメモは保存しようとした時点で、外部で内容が変更されていました。」(保存しようとしたとき)
- 「このメモは開いている間に、外部で内容が変更されました。」(開いている間)

「どちらの内容を残すか選択してください。」に従い、次のどちらかを選びます。

- 「ディスクから再読み込み（自分の変更を破棄）」: ほかのプログラムで変更された内容を読み込みます。アプリで編集していた内容は捨てられます。
- 「自分の内容を保持して上書き」: アプリで編集していた内容で、ファイルを上書きします。

このダイアログは Esc では閉じられません。必ずどちらかを選んでください。迷うときは、エクスプローラーで `メモ名.md` を別の場所にコピーしてから選ぶと安心です。

エクスプローラーなどでタブのフォルダにメモを追加・削除した場合は、ツリーに自動で反映されます(ロック中のタブを除きます)。

### 10.2 `.bak` ファイルについて

メモを保存するたびに、直前の内容が `メモ名.md.bak` として同じフォルダに1世代だけ残ります。誤って上書きしてしまったときは、この `.bak` を `.md` に名前を変えて開くと、1つ前の状態を取り戻せる場合があります。

- `.bak` はツリーには表示されません(エクスプローラーでは見えます)。
- メモを削除すると、その `.bak` もいっしょに削除されます。
- 管理用ファイル `.tabtreenote.json` や、設定ファイル `data/settings.json` にも、同じように `.bak` が作られます。

### 10.3 「管理情報を復元しました」と出たとき

タブの管理用ファイル(`.tabtreenote.json`)や設定ファイルが壊れていたときに出る通知です。まず `.bak` から復元を試み、それもできないときは、フォルダの中の `.md` ファイルから一覧を作り直します。作り直した場合は、メモの階層・色ラベルなどが失われますが、**メモの内容は失われません**。

### 10.4 エラーメッセージの意味

| 表示 | 意味と対処 |
|---|---|
| 「同じ名前のものが既に存在します」 | 同じ名前がすでにあります。別の名前にしてください。 |
| 「使用できない名前です」 | 使えない文字や名前です(2.1 参照)。 |
| 「ファイルが他のプログラムで使用されています」 | ほかのプログラムがファイルを開いています。閉じてからやり直してください。 |
| 「パスが長すぎる可能性があります」 | フォルダの階層が深いなどでパスが長すぎます(260 文字を超える長いパスには、正式には対応していません)。短い場所に置き直してください。 |
| 「パスワードが正しくありません」 | タブロックのパスワードが違います。 |
| 「ロックされたタブには操作できません」 | ロックを解除してから操作してください。 |
| 「編集ロック中のメモは削除・名前変更できません」 | 先に「編集ロック解除」を選んでください。 |
| 「画像フォルダの名前を変更できなかったため、処理を中止しました。…」 | 画像フォルダの中の画像を開いているプログラムを閉じて、もう一度試してください。 |

### 10.5 よくある疑問

**Q. メモはどこに保存されていますか。**
A. タブとして登録したフォルダの中の `メモ名.md` です。ツリーの右クリックメニューの「ファイルの場所を開く」、またはタブの「タブの場所を開く」で、エクスプローラーで開けます。

**Q. バックアップはどうすればよいですか。**
A. タブのフォルダをコピーするだけでバックアップになります(ロック中のタブでも同じです)。アプリの設定(タブの登録情報など)も残したいときは、実行ファイルと `data/` フォルダもコピーしてください。

**Q. 2つ目を起動するとどうなりますか。**
A. すでに起動しているアプリのウィンドウが前面に出ます。2つ同時には起動しません。

**Q. メモを削除してしまいました。**
A. Windows のごみ箱を確認してください。ごみ箱に移動されています(「完全に削除」を選んだ場合を除きます)。タブの「削除」は登録を外すだけなので、フォルダの中身はそのまま残っています。

**Q. 貼り付けた画像のファイルが見つかりません。**
A. 通常のタブでは、`タブのフォルダ/メモ名/` の中にあります(4.6 参照)。ロック中のタブのロック解除中に貼り付けた画像は、タブのフォルダ直下に `img_…` という名前で保存されています。

**Q. 検索に出てこないメモがあります。**
A. ロック中のタブは、検索の対象外です。また、ロックを解除して開いている間も対象外です。

**Q. 数式が表示されません。**
A. 数式の形で表示されるのは、プレビュー(「分割表示」「プレビューのみ」)だけです(4.7 参照)。

**Q. 画面の文字が小さい/大きいです。**
A. 「設定」→「一般」の「UI文字サイズ」で変えられます。エディタ本文の大きさは、「設定」→「エディタ」の「フォントサイズ」です。

**Q. 英語の画面にしたいです。**
A. 「設定」→「一般」の「言語」で「English」を選んでください。

**Q. ツールバーが邪魔です。**
A. ツールバー上部の帯をドラッグして動かすか、「ツールバーを最小化」で畳んでください。

---

<a id="english"></a>

# TabTreeNoteMD User Guide (English)

[日本語](#japanese) | **English**

This guide explains how to use TabTreeNoteMD for the first time. No programming knowledge is required. Names shown on screen (menus and buttons) are written in quotation marks.

> Screen text and behavior may change between versions. This guide is based on the v1.2.0 development branch.

## Table of Contents

1. [Introduction](#1-introduction)
2. [Managing Tabs](#2-managing-tabs)
3. [Managing Memos](#3-managing-memos)
4. [Editing Memos](#4-editing-memos)
5. [Search and Replace](#5-search-and-replace)
6. [Links](#6-links)
7. [Tab Lock](#7-tab-lock)
8. [Settings](#8-settings)
9. [Keyboard Shortcuts](#9-keyboard-shortcuts)
10. [Troubleshooting](#10-troubleshooting)

---

## 1. Introduction

### 1.1 Basic Concepts

TabTreeNoteMD is a Markdown note app for Windows that organizes memos in a "tabs × tree" layout.

- **A tab is a folder**: each tab at the top of the window corresponds to one folder on your computer.
- **A memo is an `.md` file**: each memo in a tab corresponds to one Markdown file (extension `.md`) directly inside that folder. The memo name and the file name are the same.
- **Tree**: the left side of the window shows the memos of the selected tab as a hierarchy (parent and child memos).
- **Editor**: the right side is where you edit the selected memo.

Memos stay on your computer as ordinary text files, so you can open them in other editors, or back them up by copying the folder.

> Note: the tree shows only the `.md` files **directly** inside the tab's folder. Subfolders and files that are not `.md` are not shown.

### 1.2 Screen Layout

- **Tab bar (top)**: from the left, the app name, the tabs, the "Register Existing Folder" button (📂) and the "Create New Tab" button (＋). On the right are the tool buttons (★ Favorites, 📌 Pin on Top, 🔍 search, ⇄ Switch Tree Side, ⚙ Settings) and the minimize, maximize and close buttons.
- **Tree (left)**: the list of memos.
- **Editor (right)**: where you edit the memo. The status bar at the bottom shows the character count, line count and cursor position ("Ln ○, Col ○").
- **Toolbar**: a group of formatting buttons that floats over the editor (see 4.3).

Drag the border between the tree and the editor to change the tree width. Double-click the border to hide the tree; use "Show Tree" to bring it back.

This app has no standard Windows title bar. Drag an empty part of the tab bar to move the window.

The "Close" button (×) at the top right **quits the app** (it does not go to the tray). The memo you are editing is saved just before quitting. This behavior does not depend on any setting. When you minimize, the app stays on the taskbar. A tray icon is shown, but the app is placed in the tray only when it was started through "Auto-launch to tray on Windows login" (see 8.1).

### 1.3 Where Data Is Stored (Portable Layout)

- App settings and tab registrations are stored in the **`data/` folder** next to the executable. The Windows registry and `%APPDATA%` are not used.
- If you copy only the executable to another location and start it, it runs as an independent environment with no settings or tabs. You can keep several copies side by side.
- Folders of tabs made with "Create New Tab" are created under `data/tabs/`. Folders registered with "Register Existing Folder" stay where they are.
- To move your settings to another computer, copy the executable and the `data/` folder together.

### 1.4 Autosave and Resuming

- Your edits are saved automatically. Saving also happens when you switch tabs or memos, minimize the window, exit, or press Ctrl+S. The autosave interval can be changed in Settings (see 8).
- Every time you save, the previous content is kept as a single generation in a file named `memo-name.md.bak` (see 10.2).
- On startup, the memo you had open last is opened again. When you switch tabs, the memo last open in that tab is opened and its scroll position is restored (if that memo no longer exists, nothing is opened).

### 1.5 Display Language

In "Settings" > "General", "Language" switches between "日本語" and "English".

---

## 2. Managing Tabs

### 2.1 Creating and Registering Tabs

There are three ways to add a tab.

**Method A: create a new empty tab**

1. Click "＋" (tooltip "Create New Tab") on the tab bar.
2. In the "Create New Tab" dialog, enter a name in "Tab (folder) name" and press "Create".
3. A folder with that name is created under `data/tabs/` and registered as a tab. If a folder with the same name already exists, a number is added, as in `name(1)` and `name(2)`.

**Method B: register an existing folder**

1. Click "📂" (tooltip "Register Existing Folder") on the tab bar.
2. In the Windows folder picker, choose the folder you want to register.
3. That folder becomes a tab. If the folder contains the hidden file `.tabtreenote.json` that this app creates for management, its contents (hierarchy, labels and so on) are carried over.

**Method C: drag a folder to register it**

Drag and drop a folder from File Explorer onto the tab bar (the registration is the same as Method B).

When no tab is registered, the screen shows "No tabs are registered" with the "Register Existing Folder" and "Create New Tab" buttons.

**About duplicate names**: tab names cannot be duplicated. If the name of the folder you are registering is already used, the "Duplicate Tab Name" dialog opens; enter a different name as instructed by "Enter a different name".

**Names that cannot be used**: `\ / : * ? " < > |` cannot be used in names. Names reserved by Windows, leading or trailing spaces, and a trailing period are also not allowed. The reason is shown below the input field.

### 2.2 Switching Tabs

- Click a tab.
- Ctrl+Tab goes to the next tab and Ctrl+Shift+Tab to the previous tab.

Double-clicking the active (selected) tab moves the cursor and scroll position to the start (line 1) of the open memo (hovering over the tab shows "(double-click to jump to start)"). Double-clicking again returns to the earlier position. The remembered position is discarded when you switch tabs or memos.

### 2.3 Reordering Tabs

Drag a tab with the mouse to the position you want. The order is saved.

### 2.4 Tab Context Menu

Right-click a tab to open this menu.

| Menu item | What it does |
|---|---|
| "Rename" | Changes the display name of the tab (not available for a locked tab). The folder's own name does not change. |
| "Set Lock Label" | Sets your own label to tell locked tabs apart (see 7.6). Leave it blank to clear it. |
| "Open Tab Location" | Opens the tab's folder in File Explorer. |
| "Lock Tab" / "Unlock" | Protects the tab with a password (see 7). |
| "Permanently Unlock" | Shown only for locked tabs (see 7.5). |
| "Remove Tab" | Removes the tab from the list (see 2.5). |

### 2.5 Removing a Tab

1. Right-click the tab and choose "Remove Tab".
2. The message "This removes “(tab name)” from the tab list. The folder/files are not deleted." appears. Press "Delete".

> Important: "Remove Tab" only **unregisters** the tab. The memos and images inside the folder are not deleted. You can register the folder again later with "Register Existing Folder".

A locked tab (before unlocking) cannot be removed. Unlock it first.

### 2.6 Other Tab Bar Buttons

- **★** ("Favorites"): opens the list of memos you added to favorites (see 3.8).
- **📌** ("Pin on Top"): keeps the app window above other windows. Pressing it again shows "Unpin". This setting is not saved and returns to normal at the next start.
- **🔍**: opens the "Full-Text Search" across all tabs (see 5.2).
- **⇄** ("Switch Tree Side"): swaps the tree between the left and right sides of the window.
- **⚙** ("Settings"): opens the settings screen (see 8).

---

## 3. Managing Memos

### 3.1 Creating Memos

- When the tree is empty, press the "+ Create Memo" button in the center.
- Right-click an empty area of the tree and choose "Create Memo" (it is added at the end of the list).
- Right-click a memo and choose "Create Memo" (added at the end) or "Create as Child" (added as a child of that memo).

In every case a name dialog ("Memo name") opens. Enter a name and press "Create" to create an `.md` file with that name. If a memo with the same name already exists, a number is added, as in `name(1)`.

The characters that cannot be used in memo names are the same as for tab names (see 2.1).

### 3.2 Opening and Moving Between Memos

- Click a memo to open it in the editor on the right.
- The ↑ / ↓ keys move to the previous or next memo (while the tree has focus).
- The expand/collapse mark at the left of a parent memo shows or hides its children.

### 3.3 Selecting Multiple Memos

Ctrl+click adds one memo to the selection, and Shift+click selects a range. Clicking an empty area clears the selection. Memos selected together can be deleted together or dragged together.

### 3.4 Renaming

- Right-click a memo and choose "Rename", or select a memo and press **F2**.
- Enter the name and press Enter to confirm.

Renaming a memo renames its file. For a memo with pasted images, the image folder (see 4.6) is renamed too.

> Note: when you rename a memo, `memo:` links that point to it (see 6.4) stop working. Links are not rewritten automatically, so fix them by hand if needed.

### 3.5 Reordering, Changing the Hierarchy, and Moving (Drag and Drop)

Drag a memo and drop it onto another memo.

- Dropping on the **upper part** of a memo's row puts it just before that memo.
- Dropping on the **lower part** of the row puts it just after that memo.
- Dropping on the **middle** of the row makes it a child of that memo.

**Moving or copying to another tab**: drag a memo and drop it onto another tab in the tab bar. It is moved. If you drop it **while holding Ctrl**, it is copied instead of moved.

When you move or copy a memo to another ordinary tab, its image folder (see 4.6) goes with it.

Dropping onto a locked tab opens a password dialog ("Dropped on a Locked Tab") (see 7.7).

### 3.6 Deleting Memos (Trash)

1. Right-click a memo and choose "Delete" (child memos are deleted along with it).
2. The "Delete Memo" dialog says "Move the ○ selected memo(s) (including child memos) to Trash." Press "Yes".

A deleted memo is moved to the Windows **Recycle Bin**. You can restore it from there.

- If it cannot be moved to the Recycle Bin, the "Failed to Move to Trash" dialog asks whether to use "Delete Permanently" (this cannot be undone).
- **Ctrl+Delete** (named "Force Delete" in settings) also deletes the selected memos. It works even while the editor has focus. The same confirmation dialog opens.
- If the memo has an image folder (see 4.6), it is moved to the Recycle Bin along with the memo. If only the image folder could not be deleted, the notice "The memo was deleted, but its image folder could not be deleted" appears.

### 3.7 Edit Lock (Protecting a Single Memo)

You can make an individual memo uneditable to avoid changing it by accident. No password is needed. (This is a separate feature from "Tab Lock", which uses a password.)

- To set it, right-click the memo and choose "Edit Lock". To clear it, choose "Remove Edit Lock".
- An edit-locked memo looks different in the tree (hovering over its name shows "Edit locked"), and the editor's status bar shows "🔒 Edit locked".
- The body cannot be edited. The toolbar buttons other than the rearrange button are also disabled.
- **Renaming and deleting are also blocked** ("An edit-locked memo cannot be deleted or renamed"). If a child memo is edit-locked, the parent memo cannot be deleted either.
- Setting or clearing an edit lock takes effect immediately on a memo that is already open.

### 3.8 Favorites

- Right-click a memo and choose "Add to Favorites". To remove it, choose "Remove from Favorites".
- Press "★" on the tab bar to open the "Favorites" list. Click a memo, or use ↑ / ↓ and Enter, to open that memo (switching to its tab). Clicking the star in the list removes it from favorites.
- To open the list from the keyboard, assign a key to "Open Favorites List" in "Settings" > "Shortcuts" (it is "Unbound" by default).
- Memos in locked tabs do not appear in the favorites list.

### 3.9 Icons (Color Labels)

Right-click a memo and open "Icon" to choose from "None", "🔴 Red", "🔵 Blue", "🟢 Green" and "🟡 Yellow". These are colored markers to help you tell memos apart.

### 3.10 Properties and File Location

- "Properties": shows the file name, file path, creation date, modification date and file size, plus total characters, characters (excluding line breaks/whitespace) and lines. It can also be opened from the editor's context menu.
- "Open File Location": opens the memo's file in File Explorer.

### 3.11 Importing `.md` Files from File Explorer

Drag and drop `.md` files from File Explorer onto the tree to import them as memos.

- This is a **copy**. The original files are never changed or deleted.
- Dropping on a memo's row imports the files as children of that memo (at the end of its children). Dropping on an empty area of the tree imports them at the end of the top level. The memo name is the file name without `.md`.
- Files other than `.md`, and folders, are not imported (they are skipped). If any items were skipped, a notice such as "Imported ○ item(s) (○ skipped)" appears after the import.
- If a memo with the same name already exists, the "Duplicate Name on Import" dialog lets you choose "Skip", "Overwrite" or "Rename and Import". Check "Apply this choice to all remaining duplicates" to apply the same choice to the rest. Pressing Esc in the duplicate dialog cancels the import: that item and the items not yet processed are not imported (counted as skipped). Items already decided before the Esc (those with no duplicate, or already chosen) are still imported.
- A memo that is edit-locked cannot be overwritten.

---

## 4. Editing Memos

### 4.1 View Modes

**Live view and source view**

- **Live view** (default): Markdown symbols (such as `#` and `**`) are shown as their visual result, such as headings and bold text, while you edit. The part where the cursor is shows the original symbols.
- **Source view**: you edit plain text with the Markdown symbols visible as they are.

Switch with the button at the top of the toolbar. The button shows the **name of the view it switches to** ("Source View" while in live view, "Live View" while in source view). The shortcut is Ctrl+Shift+M. Switching views never changes the text of the memo.

**Pane layout (Edit Only, Split View, Preview Only)**

Each press of the pane layout button at the top of the toolbar switches in this order.

1. "Edit Only": shows only the editor.
2. "Split View": shows the editor and the preview (the finished appearance) together.
3. "Preview Only": shows only the preview.

As with the live/source button, the button shows the **name of the layout it switches to**. The shortcut is Ctrl+Shift+P.

- Pressing "Pin to Preview" (the lock icon) keeps that memo in preview and disables the pane layout button. "Unpin Preview" reverses this.
- In "Preview Only", the toolbar minimizes itself automatically and the zoom controls ("－", "＋" and a percentage) appear (see 4.9).
- The view mode and pane layout are remembered for each memo. The initial value for new memos can be changed in Settings (see 8.3).

**Split view orientation**: in "Settings" > "Editor", "Split View Orientation" lets you choose "Side by Side" or "Top and Bottom".

### 4.2 Basic Editing

- Undo (Ctrl+Z), redo (Ctrl+Y), cut, copy, paste and select all are also available from the editor's context menu.
- Wrapping, tab width, line numbers and font size can be changed in Settings > "Editor" (see 8.3).
- "Insert Tab as spaces" chooses what the Tab key inserts (spaces or a tab character).
- The scroll position is remembered for each memo.

**Editor context menu**

| Menu item | Description |
|---|---|
| "Copy link" | Shown only when you right-click on a link (see 6.6). |
| "Undo", "Redo", "Cut", "Copy", "Paste", "Select All" | Ordinary editing operations. |
| "Line Operations" | "Move Line Up", "Move Line Down", "Duplicate Line", "Delete Line" |
| "Convert" | "Convert to Uppercase", "Convert to Lowercase", "Convert to Hiragana", "Convert to Katakana", "Convert to Half-Width", "Convert to Full-Width", "Tabs to Spaces", "Spaces to Tabs" |
| "Toggle Checkbox (☐/☑)" | Switches the ☐ or ☑ on the cursor's line. If there is none, a ☐ is inserted at the cursor. |
| "Search with Google" | Searches the selected text on Google in your default browser. |
| "Find and Replace" | Opens in-memo search and replace (see 5.1). |
| "Show Whitespace" | Toggles the display of whitespace and line-break marks ("Hide Whitespace" turns it off). |
| "Properties" | Shows file information and character counts. |

In an edit-locked memo, menu items that change the content are disabled.

### 4.3 Toolbar

A "toolbar" of buttons for applying formatting floats over the editor (9 columns × 2 rows). Hover over a button to see its name. Buttons act on the selected text, or at the cursor position.

| Button | What it does |
|---|---|
| "Undo", "Redo" | Cancels or repeats the previous operation |
| "Text Color" | Opens a list of colors ("Red", "Blue", "Green", "Orange", "Purple"). Applies the chosen color to the selection. "Clear" removes the color. |
| "Font Size" | Opens a list of "Small", "Large" and "Extra Large". "Clear" restores the size. |
| "Heading" | Cycles the heading level of the cursor's line: no heading → 1 → 2 → … → 6 → no heading. |
| "List" | Adds or removes "- " (a bullet) at the start of the selected lines. |
| "Bold", "Italic", "Strikethrough" | Applies or removes them on the selection. |
| "Quote" | Adds or removes "> " at the start of the selected lines. |
| "Insert Link" | Inserts a link in the form `[Link](…)`. |
| "Code Block" | Wraps the selection in a code block. With no selection, inserts an empty code block. |
| "Update Table of Contents" | Creates a table of contents (see 4.8). |
| "Insert Table" | Inserts a 2-column table template at the cursor. |
| "Insert Image" | Chooses an image file and inserts it (see 4.6). |
| "Clear Formatting" | Removes formatting from the selection. |
| "Hard Break" | Inserts a line break with two trailing spaces (a Markdown hard break). |
| "Rearrange Mode" | Enters a mode where you can drag buttons to reorder them. Press Esc or this button again to finish. The order is saved. |

The strip at the top of the toolbar (the part with "⋮⋮") also offers these operations.

- Drag the strip to move the toolbar. The position is saved.
- "Minimize Toolbar" / "Restore Toolbar" folds or unfolds the group of buttons.
- "Export PDF" (Ctrl+P): creates a PDF that looks like the preview. A screen to choose where to save opens. It cannot be pressed while no memo is open.
- The live/source switch and the pane layout switch (see 4.1).

Many of these buttons have keyboard shortcuts (see 9).

### 4.4 Writing Markdown (Basics)

CommonMark-compatible Markdown can be used: headings, lists, quotes, code blocks, tables, bold and italic, horizontal rules, links and so on.

| How to write | Result |
|---|---|
| `# Heading` | Heading (the number of `#` sets the level, up to 6) |
| `**bold**` / `*italic*` / `~~strikethrough~~` | Bold / italic / strikethrough |
| `- item` | Bullet list |
| `> quote` | Quote |
| `[text](destination)` | Link |
| `![description](image path)` | Image |

Because Markdown has no standard way to write text color and text size, they are saved in the form `<span style="color:…;font-size:…">` (this is inserted automatically when you set them from the toolbar).

> For safety, the preview removes HTML other than the allowed forms (`<img>` and `<span style>` containing only color and size). `<script>` and the like do not run.

### 4.5 Checkboxes

- **Click to toggle**: in live view, clicking a checkbox written as `- [ ]` or `- [x]` checks or unchecks it (hovering shows "Click to check" / "Click to uncheck"). It does not toggle in an edit-locked memo.
- **Right-click to toggle**: on the cursor's line, choose "Toggle Checkbox (☐/☑)" in the context menu to switch between ☐ and ☑.

### 4.6 Pasting and Inserting Images

You can add an image to a memo in any of these ways.

- Paste an image from the clipboard into the editor (Ctrl+V or "Paste" in the context menu).
- Choose an image file with the toolbar's "Insert Image".
- Drag and drop an image file from File Explorer onto the editor.

Supported image types are png, jpg, jpeg, gif, webp and bmp.

- In a normal tab, images are saved in an image folder named **`tab-folder/memo-name/`**, as sequential files `001.png`, `002.png` and so on. If you rename the memo, this folder is renamed too.
- While a locked tab is unlocked, images are saved directly in the tab's folder with random names (`img_…`), so that the memo name is not visible from outside.
- A large image may be automatically shrunk or compressed before saving (an image whose longer edge exceeds 2000 pixels is shrunk, and an image larger than 1 MB is converted to WebP to make it lighter. GIFs are not converted).
- Removing the image text from a memo's body does **not** delete the image file automatically. Tidy up unneeded images by hand in File Explorer (for what happens when you delete the memo itself, see 3.6).
- Clicking an image in the preview enlarges it in the "Image" dialog.

### 4.7 Math

You can write math using TeX notation.

- Inline math: `$formula$`
- Block math: `$$formula$$`

Formulas are displayed as typeset math only in the **preview** ("Split View" and "Preview Only"). In live view and source view, the formula part just gets a marker and is not shown in math form.

> Note: if you use "$" as an ordinary character, such as for amounts of money, it may be treated as math.

### 4.8 Table of Contents

1. Write your memo using headings (`#` through `######`).
2. Place the cursor where you want the table of contents and press "Update Table of Contents" (Ctrl+Shift+O) on the toolbar.
3. Links to the headings are inserted as a nested bullet list. The part between `<!-- TOC start -->` and `<!-- TOC end -->` is the table of contents.

The table of contents is updated **only when you press the button**. After changing headings, press "Update Table of Contents" again. If a table of contents already exists, its contents are replaced. Clicking a link in the table of contents jumps to the matching heading in that memo.

### 4.9 Preview Zoom and PDF Export

- Only in "Preview Only", the top of the toolbar shows "－", "＋" and the current magnification (%). It can be changed from 70% to 180% in steps of 10%. Clicking the magnification display returns to 100%.
- From the keyboard, Ctrl+= zooms in and Ctrl+- zooms out (changeable in Settings; Ctrl+"+"/"-" on the numeric keypad also work).
- The zoom level is not saved. It returns to 100% when you switch tabs or memos, or leave "Preview Only".
- The preview in split view is always shown at 100%.

---

## 5. Search and Replace

### 5.1 Searching and Replacing Within a Memo

1. Open a memo and press Ctrl+F (or choose "Find and Replace" from the context menu).
2. Enter the text to find in the upper field. Matches are searched as you type. Use "▼" ("Next"), "▲" ("Previous"), or Enter / Shift+Enter to move to the next or previous match. "Select All" selects every match.
3. To replace, enter text in the lower field ("Replacement text"), then press "Replace" to replace one at a time or "Replace All" to replace everything. Pressing Enter in the replacement field also replaces one match.
4. Options: "Case sensitive", "Whole word" and "Regex".
5. Close the search panel with "×" ("Close (Esc)") or Esc.

Past search terms appear as suggestions in the input field.

### 5.2 Searching Across All Tabs (Full-Text Search)

1. Press "🔍" on the tab bar. The "Full-Text Search" dialog opens.
2. Enter the term in "Enter a search term..." and press "Search" (or Enter).
3. Choose the range from "All Tabs" and "Current Tab" (the initial value is "Current Tab"). You can also choose "Case sensitive" and "Regex".
4. The results list shows the memo name, tab name and where it matched ("Name", "Body" or "Name & Body"). Click a result to open that memo in its tab.

- The search covers memo names and bodies.
- **Locked tabs are not searched (even while temporarily unlocked).**
- When there are no results, "No matching results" appears.

### 5.3 Quick Open (Open a Memo by Name Quickly)

1. Press **Ctrl+O**. The "Quick Open" dialog opens.
2. Type part of a memo name in "Search by memo name...". Memos from all tabs appear as candidates, each with its memo name and tab name.
3. Choose a candidate with ↑ / ↓ and press Enter to open it. Clicking also opens it.

The star (☆/★) at the right of a candidate adds or removes that memo from favorites. Memos in locked tabs do not appear as candidates. The shortcut key can be changed in Settings.

---

## 6. Links

### 6.1 Kinds of Links

In the editor (live view) and the preview, you can click these links to open them.

| Kind | Example | What opens |
|---|---|---|
| URL | `https://example.com` or `www.example.com` | Your default browser |
| Email | `name@example.com` | Your default mail app |
| File path | `C:\Users\…\memo.txt`, `./relative/path/file` | The file, in its associated app |
| FTP | `[display text](ftp://example.com/file)` | The default app (whatever the OS associates with `ftp://`) |
| Memo link | `memo:memo-name` and so on (see 6.4) | The memo, inside the app |
| Heading link | `[Heading](#heading-name)` (created by the table of contents) | Jumps to the heading within the same memo |

- Links can also be written in the form `[display text](destination)`.
- An `ftp://` link written as `[display text](ftp://…)` can be opened in both the live view and the preview (it is handed to the OS's default association). If you write only `ftp://…`, the preview turns it into a link automatically, but the live view does not.
- URLs and email addresses containing Japanese are supported. If a Japanese punctuation mark (such as "。" or "、") appears in the middle of a URL, the URL is treated as ending there.
- A relative file path (starting with `./` or `../`) is looked up relative to the folder of the memo's tab.
- A file that cannot be found, or a `memo:` link that cannot be resolved, does nothing.

### 6.2 How Links Are Opened (Settings)

Set these in "Settings" > "General" > "Links".

- "Open Trigger": choose "Single Click" (default), "Ctrl+Click" or "Double Click".
- "Confirm before opening": when on, the "Open External Link" dialog asks you to confirm before a link opens.

### 6.3 Executable Files Always Ask for Confirmation

If a file-path link points to an executable or a script (`.exe`, `.bat`, `.ps1` and so on), the "Open Executable File" dialog **always** asks for confirmation, even when "Confirm before opening" is off. It says "This will run an executable file or script. Do not run it unless you trust it." Only open files you recognize.

### 6.4 Links Between Memos (`memo:`)

You can link from one memo to another.

| How to write | Link target |
|---|---|
| `memo:memo-name` | That memo in the same tab |
| `memo:tab-name/memo-name` | That memo in the specified tab |
| `memo:tab-name/` | The specified tab |
| `[display text](memo:tab-name/memo-name)` | A link with display text |
| `memo:tab-name/memo-name#heading-name` | A specific heading in that memo (the part after `#` is written the same way as heading names in table of contents links) |

- Memos in a locked tab (while it is not unlocked) are treated as unresolved links. While unlocked, they open normally.
- Renaming a memo breaks links to it (see 3.4).

### 6.5 Heading Links

The `[Heading](#heading-name)` links created by the table of contents (4.8) jump to the matching heading in the same memo. If no heading matches, nothing happens.

### 6.6 Copying a Link

Right-click on a URL, email or file-path link and "Copy link" appears at the top of the menu. It copies the link destination to the clipboard (in both the editor and the preview).

---

## 7. Tab Lock

### 7.1 What Tab Lock Is

Tab Lock protects **the whole contents of a tab** with a password. A locked tab hides its memo list, memo contents and the tab name (shown as 🔒 instead).

- There is no per-memo password protection. Only a whole tab can be protected.
- The encryption method is AES-256-GCM (scrypt is used to derive the key from the password).
- Locked tabs are excluded from full-text search, Quick Open and the favorites list. `memo:` links to memos in a locked tab do not open either.
- A locked tab cannot be removed (unlock it first).

> Note: when you lock a tab, the `.md` files in the tab's folder are **encrypted and their file names change to random strings**. Even if you open the folder in File Explorer, you cannot read the original memo names or contents. Before using locks, please also read the cautions in 7.8.

### 7.2 Locking a Tab

1. Right-click the tab and choose "Lock Tab".
2. In the "Lock “(tab name)”" dialog, enter "Password" and "Confirm Password". In "Hint (optional)" you can enter a clue to help you remember the password.
3. Press "Lock".

- If the two passwords differ, "Passwords do not match" appears.
- A memo open in that tab is saved and then closed.
- When locking finishes, the tab shows 🔒. The tree shows "🔒 This tab is locked" and an "🔓 Unlock" button.

### 7.3 Unlocking (Opening Temporarily)

1. Press the "🔓 Unlock" button, or right-click the tab and choose "Unlock".
2. Enter the password and press "Unlock". If you set a hint, the input screen shows "Hint: …".
3. With a wrong password, "The password is incorrect" appears.

After a successful unlock, the real tab name and the memo list become visible, and you can edit as in an ordinary tab. At this point "🔓 This tab is temporarily unlocked" is shown above the tree.

This state is **temporary only**, and the tab is locked again automatically in these cases.

- When you switch to another tab.
- When the app window loses focus to another app.
- When there has been no keyboard or mouse activity for a set time (see 7.4).

To open it again you must enter the password every time. Edits in progress are saved first when the tab is locked.

### 7.4 Locking Automatically After Inactivity

In "Settings" > "General" > "Tab Lock" > "Auto-lock on Inactivity", choose from "5 min", "15 min", "30 min", "60 min" and "Disabled" (the initial value is 15 minutes).

- About 30 seconds before locking, "This tab will lock soon due to inactivity" is shown. After locking, "Locked automatically due to inactivity" is shown.
- While a dialog is open, the countdown pauses.
- While the file picker for the toolbar's "Insert Image" is open in an unlocked tab (for up to 2 minutes), losing focus does not lock the tab right away.

### 7.5 Permanently Unlocking (Returning to an Ordinary Tab)

This operation ends password protection altogether and returns the tab to an ordinary tab.

- Right-click a locked tab and choose "Permanently Unlock".
- Or, while the lock is temporarily open, press the "Permanently Unlock" button at the top of the tree.

A confirmation dialog says "Permanently unlocking … returns it to a normal, unencrypted tab. This cannot be undone." As it says, "As a precaution, we recommend backing up the entire folder beforehand", so copying the folder first is a good idea. When it runs, all memos are decrypted and file names return to the original memo names. When run from a locked tab, you must enter the password.

> Even if you lock the tab again after permanently unlocking it, the fact that it was unencrypted in the meantime cannot be undone.

### 7.6 Labels to Tell Locked Tabs Apart

A locked tab hides its name, so every locked tab shows only 🔒. Right-click the tab and choose "Set Lock Label" to give it a label of your own ("Label (leave blank to clear)"). A locked tab then shows "🔒 label".

> Note: the label is saved **unencrypted, as plain text**, in the app's settings file (`data/settings.json`). It can be read even while the tab is locked, and by anyone who looks at the settings file. The label is for telling locked tabs apart. Do not put words you do not want others to know, such as the real tab name or anything about the memo contents.

### 7.7 Exchanging Memos With Other Tabs

- Drag a memo from another tab and drop it onto a locked tab, and the "Dropped on a Locked Tab" dialog opens. Enter the password and press "Unlock and Move" to move the memo (or copy it, if you held Ctrl). If you close the dialog without entering a password, nothing happens.
- You can also move a memo from a temporarily unlocked tab to another ordinary tab. The moved memo becomes an ordinary, **unencrypted** `.md` file.

### 7.8 Cautions

- **This app has no feature to recover a forgotten password.** The contents can only be read with the password, and there is no way to reissue the password or to unlock the tab on someone's behalf. Manage your password so that you do not forget it.
- The "hint" is saved **unencrypted, as plain text**, in `.tabtreenote.json` inside the tab's folder (so that it can be shown on the screen before the password is entered). Anyone who looks at the folder can read it. Do not put the password itself, or words that make it easy to guess, in the hint.
- Before using locks, or before permanently unlocking, it is a good idea to back up by copying the whole folder (copying the folder works even while the tab is locked).
- If locking is interrupted, for example because the app stops midway, opening that tab shows "The previous lock operation did not finish. Recovery is required." In the "Lock Recovery" dialog, enter the password and choose "Finish Locking" or "Undo (cancel the lock)". If a permanent unlock is interrupted, you can similarly continue or restore the lock from the "Permanent Unlock Recovery" screen. If "files in an unexpected state were found" appears, manual review is required.
- A locked tab cannot be renamed until it is unlocked.

---

## 8. Settings

Press "⚙" ("Settings") on the tab bar to open the "Settings" screen. Switch screens with the "General", "Shortcuts" and "Editor" tabs at the top. Changes take effect immediately; there is no "Apply" step. Close it with "Close".

### 8.1 General

**Appearance**

- "Theme": "Light", "Dark"
- "UI Font Size (○%)": changes the size of on-screen text (other than the editor body) from 80% to 150%.
- "Accent Color": chooses the highlight color. "↺" ("Reset to default") returns to the initial color.
- "UI Font": enter the font name for on-screen text. "↺" returns to the initial font.
- "Tree Row Line Height (○)": changes the height of tree rows.
- "Language": "日本語", "English"

**Links** (see 6.2)

- "Open Trigger": "Single Click", "Ctrl+Click", "Double Click"
- "Confirm before opening" (links to executables and scripts always ask for confirmation, even when this is off)

**Startup**

- "Auto-launch to tray on Windows login": when on, the app starts in the task tray when you log in to Windows. Click the tray icon to show the window. Note that the window's "Close" button (×) quits the app regardless of this setting (it does not go to the tray).

**Tab Lock** (see 7.4)

- "Auto-lock on Inactivity": "5 min", "15 min", "30 min", "60 min", "Disabled"

**Third-Party Licenses**

Press "Third-Party Licenses" at the very bottom to open the license notices of the software this app uses.

### 8.2 Shortcuts

A list of the keyboard shortcut for each operation (see 9 for the default keys). Keys can be changed (except "Next Tab" and "Previous Tab", for which only the default keys are currently active; see 9).

1. Press the key button in the row of the operation you want to change. "Waiting for key input..." appears.
2. Press the key combination you want to assign (Ctrl or Shift alone does not confirm it; press a regular key too).
3. Press Esc to cancel.
4. A changed row shows "↺" ("Reset to default"). Press it to return to the initial key.

An operation with no key assigned shows "Unbound".

### 8.3 Editor

**Editor Defaults**

- "Wrap": "Wrap at window width", "Wrap at character count", "No wrap"
- "Tab Width" (1 to 16)
- "Insert Tab as spaces"
- "Show line numbers"
- "Font Size" (8 to 40)
- "Default Pane Layout for New Memos": "Edit Only", "Split View", "Preview Only"
- "Split View Orientation": "Side by Side", "Top and Bottom"

**Preview**

- "Preview redraw delay (ms)": the wait between typing and the preview updating (50 to 2000).

**Autosave**

- "Autosave delay (ms)": the wait between when typing stops and the automatic save (200 to 10000).

**Line Endings**

- "Save Policy": "Preserve format on load" (default), "CRLF", "LF", "CR"
- "Encoding": "UTF-8 (fixed)" (cannot be changed)

---

## 9. Keyboard Shortcuts

The tables below show the initial keys. They can be changed in "Settings" > "Shortcuts" (see 8.2). However, "Next Tab" and "Previous Tab" are listed on the settings screen but currently do not take effect when changed; they stay Ctrl+Tab / Ctrl+Shift+Tab.

**Tabs, tree and search**

| Operation | Default key |
|---|---|
| Next Tab | Ctrl+Tab |
| Previous Tab | Ctrl+Shift+Tab |
| Save | Ctrl+S |
| Open Find/Replace | Ctrl+F |
| Rename (the memo selected in the tree) | F2 |
| Force Delete (deletes the memos selected in the tree) | Ctrl+Delete |
| Quick Open | Ctrl+O |
| Open Favorites List | Unbound (assign your own) |

**Formatting and display**

| Operation | Default key |
|---|---|
| Undo | Ctrl+Z |
| Redo | Ctrl+Y |
| Text Color | Ctrl+Shift+C |
| Font Size | Ctrl+Shift+F |
| Heading | Ctrl+Shift+H |
| List | Ctrl+Shift+L |
| Bold | Ctrl+B |
| Italic | Ctrl+I |
| Strikethrough | Ctrl+Shift+X |
| Quote | Ctrl+Shift+Q |
| Insert Link | Ctrl+K |
| Code Block | Ctrl+Shift+K |
| Update Table of Contents | Ctrl+Shift+O |
| Insert Table | Ctrl+Shift+T |
| Insert Image | Ctrl+Shift+I |
| Clear Formatting | Ctrl+Shift+E |
| Hard Break | Shift+Enter |
| Rearrange Toolbar | Ctrl+Shift+R |
| Toggle Live/Source View | Ctrl+Shift+M |
| Cycle Pane Layout | Ctrl+Shift+P |
| Export PDF | Ctrl+P |
| Zoom Preview In | Ctrl+= |
| Zoom Preview Out | Ctrl+- |

**Others (keys that cannot be changed)**

| Operation | Key |
|---|---|
| Move to the previous/next memo in the tree | ↑ / ↓ |
| Next/previous match in the search panel | Enter / Shift+Enter |
| Close the search panel or a dialog | Esc |
| Preview zoom (numeric keypad) | Ctrl+numeric keypad "+" / "-" |

---

## 10. Troubleshooting

### 10.1 When a Memo Is Changed by Another Program (Conflict Prompt)

If another program rewrites a memo's file while you have it open, the dialog "The file has been changed externally" opens.

- "This memo's content was changed externally by the time the save was attempted." (when you tried to save)
- "This memo's content was changed externally while it was open." (while it was open)

Follow "Please choose which content to keep." and choose one of these.

- "Reload from Disk (discard my changes)": loads the content changed by the other program. What you were editing in the app is discarded.
- "Keep My Content and Overwrite": overwrites the file with what you were editing in the app.

This dialog cannot be closed with Esc. You must choose one. If you are unsure, copy `memo-name.md` to another place in File Explorer before choosing.

When you add or delete memos in a tab's folder with File Explorer or similar, the tree updates automatically (except for locked tabs).

### 10.2 About `.bak` Files

Each time a memo is saved, the previous content is kept in the same folder as `memo-name.md.bak`, as a single generation. If you overwrite something by mistake, renaming that `.bak` to `.md` and opening it may bring back the previous state.

- `.bak` files are not shown in the tree (they are visible in File Explorer).
- When you delete a memo, its `.bak` is deleted along with it.
- The management file `.tabtreenote.json` and the settings file `data/settings.json` get `.bak` files in the same way.

### 10.3 When "Restored the management metadata" Appears

This notice appears when a tab's management file (`.tabtreenote.json`) or the settings file was damaged. The app first tries to restore from the `.bak`; if that is not possible, it rebuilds the list from the `.md` files in the folder. When rebuilt, the memo hierarchy, color labels and so on are lost, but **the contents of your memos are not lost**.

### 10.4 What the Error Messages Mean

| Message | Meaning and what to do |
|---|---|
| "An item with the same name already exists" | The name is already used. Choose a different name. |
| "This name cannot be used" | It contains characters or a name that cannot be used (see 2.1). |
| "The file is in use by another program" | Another program has the file open. Close it and try again. |
| "The path may be too long" | The path is too long, for example because folders are nested deeply (paths longer than 260 characters are not formally supported). Move it to a shorter location. |
| "The password is incorrect" | The Tab Lock password is wrong. |
| "This operation cannot be performed on a locked tab" | Unlock the tab first. |
| "An edit-locked memo cannot be deleted or renamed" | Choose "Remove Edit Lock" first. |
| "An image folder could not be renamed, so the operation was stopped. …" | Close any program that is using images in the folder, then try again. |

### 10.5 Frequently Asked Questions

**Q. Where are my memos saved?**
A. They are `memo-name.md` files inside the folder you registered as a tab. Use "Open File Location" in the tree's context menu, or "Open Tab Location" on the tab, to open them in File Explorer.

**Q. How do I make a backup?**
A. Just copy the tab's folder (this works for locked tabs too). To also keep the app's settings (such as tab registrations), copy the executable and the `data/` folder as well.

**Q. What happens if I start a second copy?**
A. The window of the already-running app comes to the front. Two copies do not run at the same time.

**Q. I deleted a memo by mistake.**
A. Check the Windows Recycle Bin. The memo was moved there (unless you chose "Delete Permanently"). "Remove Tab" only unregisters the tab, so the contents of its folder remain as they were.

**Q. I cannot find the pasted image file.**
A. In a normal tab it is inside `tab-folder/memo-name/` (see 4.6). An image pasted while a locked tab was unlocked is saved directly in the tab's folder with a name like `img_…`.

**Q. Some memos do not show up in search.**
A. Locked tabs are not searched, even while they are temporarily unlocked.

**Q. Math is not displayed.**
A. Math is displayed as formulas only in the preview ("Split View" and "Preview Only") (see 4.7).

**Q. The on-screen text is too small or too large.**
A. Change it with "UI Font Size" in "Settings" > "General". The size of the editor body is "Font Size" in "Settings" > "Editor".

**Q. I want the screen in Japanese.**
A. Choose "日本語" for "Language" in "Settings" > "General".

**Q. The toolbar is in the way.**
A. Drag the strip at the top of the toolbar to move it, or fold it with "Minimize Toolbar".
