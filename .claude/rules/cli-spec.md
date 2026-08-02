# CLIコマンド仕様

## ノート参照(`<ref>`)

`show` / `edit` / `rm` / `tag` / `status` などノート1件を指定するコマンドは、frontmatterの`id`(ULID)の**後方一致**で解決する。曖昧(複数マッチ)な場合は候補一覧を表示してエラー終了する。

前方一致ではなく後方一致にしている理由: ULIDの先頭48bit(10文字相当)はタイムスタンプなので、近い時刻に作ったノート同士は先頭が一致してしまう(クイックキャプチャを連続で使うと頻発する)。末尾はランダム部分なので衝突しにくい。`list`表示の短縮IDも末尾7文字を使う。

## コマンド一覧

| コマンド | 役割 |
|---|---|
| `engnote capture <text>` | クイックキャプチャ。status=draftで即保存。`context_path`はデフォルト未設定(閉じた世界)。`--here`で今いる場所に紐付ける |
| `engnote new [--title T] [--tag T]... [--here] [--context PATH]` | `$EDITOR`を開いてじっくり書く。`context_path`はデフォルト未設定、`--here`または`--context`で明示的に設定 |
| `engnote list [--status S] [--tag T] [--context PATH]` | ノート一覧(フィルタ可) |
| `engnote here [--status S]` | `list --context <今いる場所またはその祖先>`のショートカット |
| `engnote show <ref>` | ノート本文を表示 |
| `engnote edit <ref>` | `$EDITOR`で開く(保存時に`updated`を自動更新) |
| `engnote rm <ref>` | 削除(確認あり) |
| `engnote tag <ref> add\|rm <tag>` | タグ操作 |
| `engnote status <ref> <draft\|active\|archived>` | ステータス変更 |
| `engnote search <query>` | 全文検索(tantivy) |
| `engnote sync` | vaultのMarkdownからSQLiteインデックスを再構築(エディタ直接編集/git pull後用) |
