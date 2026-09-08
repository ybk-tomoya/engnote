# SQLiteインデックス仕様

vaultのMarkdownファイルが正、SQLiteとtantivyは派生インデックス([[storage.md]]参照)。本文は複製せずtantivy側だけが持つ。SQLiteはメタデータ/フィルタ用、tantivyは全文検索専用と役割を分ける。

## スキーマ

```sql
CREATE TABLE notes (
    id TEXT PRIMARY KEY,       -- ULID
    path TEXT NOT NULL,        -- vault内の相対ファイルパス
    title TEXT NOT NULL,
    context_path TEXT,         -- NULL可
    created TEXT NOT NULL,     -- ISO8601
    updated TEXT NOT NULL,
    file_mtime INTEGER NOT NULL,
    file_size INTEGER NOT NULL
);

CREATE TABLE tags (
    note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (note_id, tag)
);

CREATE TABLE links (
    from_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    to_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    PRIMARY KEY (from_id, to_id)
);
```

## 更新方式

- 書き込み系コマンド(`capture` / `new` / `edit` / `tag` / `rm`)はファイルを書いた直後に、そのノートの分だけSQLite/tantivyを直接更新する。全件走査はしない。
- `engnote sync`は、engnoteを介さない変更(エディタでの直接編集、`git pull`/ブランチ切替など)を拾うための整合性チェック専用コマンド。
  - vault内の`.md`を全走査し、各ファイルの`file_mtime`+`file_size`をインデックス側の値と比較(内容ハッシュより軽量)
  - 差分があるファイルだけ再パースしてSQLite/tantivyを更新
  - インデックスにあるがファイルが存在しないものは削除
