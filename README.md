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
- `[[note]]`リンクの解決

詳しい仕様は `.claude/rules/` を参照。
