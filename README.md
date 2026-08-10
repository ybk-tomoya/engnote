# engnote

Notionに閉じずローカルのディレクトリ構造とも紐付けられる、エンジニア向けの万能メモアプリ(開発中)。

## インストール

```bash
cargo install --path crates/engnote-cli
```

`~/.cargo/bin`にPATHが通っていれば`engnote`コマンドが使えるようになる。コード変更後は再度実行すれば入れ直せる。

インストールせず動かす場合は `cargo run -p engnote-cli -- <サブコマンド>`。

### シェル補完(zsh)

`~/.zshrc` に1行追加すると、`show`/`edit`/`rm`/`tag`/`status` の `<ref>` をTabで補完できる(ノートのタイトル・タグが候補として出る)。

```bash
echo 'source <(COMPLETE=zsh engnote)' >> ~/.zshrc
```

`engnote`をアップグレードしたら`exec zsh`などでシェルを再起動すること(補完スクリプトは`engnote`本体から都度生成される)。

## GUI(macOS)

`crates/engnote-gui` はegui/eframeによるネイティブGUI。Dockアイコンから起動できる`.app`として使うには:

```bash
cargo install cargo-bundle  # 初回のみ
cd crates/engnote-gui
cargo bundle --release
rm -rf /Applications/engnote.app
cp -R ../../target/release/bundle/osx/engnote.app /Applications/
codesign --force --deep --sign - /Applications/engnote.app
```

最後の`codesign`は必須。`cargo bundle`が作るバンドルはバイナリ単体は署名済みだがバンドル全体(`Info.plist`・アイコン)には署名が及んでおらず、これが無いとDockが起動中のアイコンをうまく解決できず、実行ファイル名の頭文字だけの汎用アイコンにフォールバックしてしまう。

`/Applications/engnote.app` をDockにドラッグすれば、アイコンクリックで起動できる。コード変更後は同じ手順で入れ直す。

アイコンは `crates/engnote-gui/icons/icon-1024.png` が元データ。変更した場合は `crates/engnote-gui/icons/generate-icns.sh` で `AppIcon.icns` を再生成する(macOS標準の`sips`/`iconutil`を使用)。

インストールせず動かす場合は `cargo run -p engnote-gui`。

### タブ・グループ

中央パネルの上部にタブバー(VSCode/ブラウザ風)、画面最左に縦のグループレール(ブラウザの「タブグループ」風)がある。1グループ = 横並びのタブの束で、複数ノートを同時に開いて見比べながら作業できる。

- タブはノート1件分の表示/編集状態を持つ。同じタブのまま別ノートを選ぶと表示が差し替わり、新しいタブで開きたいときは明示的に新規タブを作る
- グループを閉じるにはそのグループの最後の1タブを閉じる(タブが0のグループは残らない)。グループレールの右クリックでも直接閉じられる(グループが2つ以上あるとき)
- 編集中のタブはタブバーに「✎ タイトル」で表示され、別タブ/別グループに切り替えても編集内容は保持される(保存し忘れて閉じない限り消えない)

| キー | 動作 |
|---|---|
| `Cmd+T` / `Ctrl+T` | アクティブなグループに新規タブを開く |
| `Cmd+W` / `Ctrl+W` | アクティブなタブを閉じる(最後の1枚ならグループごと閉じる) |
| `Cmd+P` / `Ctrl+P` | クイックスイッチャーを開く(下記) |
| `Ctrl+H` / `Ctrl+L` | 同じグループ内でタブを左/右に切り替え |
| `Ctrl+J` / `Ctrl+K` | グループを切り替え |

**クイックスイッチャー(Cmd+P)** — タイトル・タグでノートを検索するオーバーレイ。入力して絞り込み、`↓`/`↑`で選択、`Enter`またはクリックでアクティブなタブに開く。`Esc`で閉じる。

### キーボード操作(vim風)

タイトル・タグ・絞り込みの入力欄以外にフォーカスがある間、以下のキーが使える。

**一覧画面**

| キー | 動作 |
|---|---|
| `j` / `k` | 一覧を下/上に移動 |
| `n` | 新規ノート |
| `e` / `i` | 選択中のノートを編集 |
| `/` | 絞り込み欄にフォーカス(`Esc`で戻る) |
| `:` | 画面下部にコマンドラインを開く(`Esc`でキャンセル、`Enter`で実行) |

**コマンドライン** — `:`に続けて入力し`Enter`。

| コマンド | 動作 |
|---|---|
| `:e` / `:new` / `:enew` | 新規ノート |
| `:w` / `:wq` / `:x` | 保存 |
| `:tabnew` / `:tabe` | 新規タブ(`Cmd+T`と同じ) |
| `:tabclose` / `:tabc` | タブを閉じる(`Cmd+W`と同じ) |

**編集画面** — タイトル・タグ・本文のどれも、vimと同様Normal/Insertモードを切り替えて操作する(現在のモードは画面下部のステータスバーに表示)。今どのフィールドが対象かはハイライト・カーソル表示で分かる。

| キー | 動作 |
|---|---|
| `h` `j` `k` `l` | カーソル移動(タイトル・タグでは1行しかないので`j`/`k`はフィールド間移動になる) |
| `w` / `b` | 単語単位で前/後に移動 |
| `0` / `$` | 行(タイトル・タグではフィールド)の先頭/末尾に移動 |
| `gg` / `G` | 文書の先頭/末尾に移動 |
| `i` / `a` | カーソル位置/直後からInsertモード |
| `I` / `A` | 行(フィールド)の先頭/末尾からInsertモード |
| `o` / `O` | 下/上に新しい行を挿入してInsertモード(本文のみ) |
| `x` | カーソル位置の1文字を削除 |
| `dd` | 現在行を削除(タイトル・タグではフィールド全体をクリア) |
| `Tab` / `Shift+Tab` | タイトル→タグ→本文の順にフィールドを移動 |
| `Esc` | Insert→Normal、Normalでは編集をキャンセルして一覧に戻る |
| `Cmd+S` / `Ctrl+S` | 保存 |

マウスで非アクティブなフィールドをクリックしてもそのフィールドがアクティブになる(タイピングはNormalモードから`i`などで開始)。

**`/`コマンド(本文のみ)** — Insertモードで本文の行頭に`/`を打つとNotion風のブロック挿入メニューが開く。続けて文字を打つと絞り込み、`↓`/`↑`で選択、`Enter`または候補クリックで確定してMarkdownスニペットを挿入する(`Esc`でメニューを閉じるだけ、Insertモード自体は抜けない)。見出し(`#`〜`###`)・箇条書き・番号付きリスト・チェックボックス・引用・コードブロック・区切り線・`[[リンク]]`に対応。

### 表示画面

ノート表示中の本文はMarkdownとしてレンダリングされる(見出し・太字・箇条書き・チェックボックス・インラインコード/コードブロックに対応)。

`[[ノートタイトル]]`または`[[...ULID末尾]]`と書くと他のノートへのリンクになる(.claude/rules/linking.md参照)。表示画面では本文の下に「リンク」(このノートから参照しているノート)と「バックリンク」(このノートを参照しているノート)が一覧表示され、クリックでそのノートに移動できる。未解決(タイトル不一致・重複)のリンクは一覧に出ない。

## 設定

- `ENGNOTE_VAULT_DIR`: ノートの保存先。未設定なら`~/.engnote/vault`
- `EDITOR`: `new`/`edit`で開くエディタ。未設定なら`vi`

## 使い方

```bash
# クイックキャプチャ(status=draft、閉じた世界)
engnote capture "SQLiteのインデックス設計で詰まった"

# 今いるディレクトリに紐付けて保存(開いた世界)
engnote capture "engnoteのCLI雛形をつくった" --here

# $EDITORでじっくり書く
engnote new --title "設計メモ" --tag rust --tag architecture --here

# 一覧・絞り込み
engnote list
engnote list --status draft
engnote list --tag rust
engnote here                 # 今いるディレクトリに関連するノートだけ

# 個別ノート操作(<ref> は id の末尾数文字、またはタイトルの部分一致。省略すると一覧から絞り込み選択)
engnote show [<ref>]
engnote edit [<ref>]
engnote tag [<ref>] add rust
engnote tag [<ref>] rm rust
engnote status [<ref>] active   # draft | active | archived
engnote rm [<ref>]              # 確認あり
```

## まだ無い機能

- `search`(全文検索)
- `sync`(SQLiteインデックス再構築、エディタでの直接編集/git pull後の整合性チェック)
- CLIでの`[[note]]`リンク解決・`--broken-links`(GUIでのリンク表示/バックリンクのみ実装済み)

詳しい仕様は `.claude/rules/` を参照。
