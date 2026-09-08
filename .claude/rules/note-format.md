# ノートのファイル仕様

## ファイル名

`YYYYMMDD-HHMMSS-{タイトル}.md`

- タイトルは日本語を含めそのまま使う(APFS/ext4等はUTF-8ファイル名に問題なし)
- タイトル中の `/ \ : * ? " < > |` は除去し、50文字程度で切り詰める
- タイトル未設定時はスラグ部分を省略し `YYYYMMDD-HHMMSS.md` とする
- ファイル名はあくまで人間が`ls`/`grep`で見当をつけるためのもの。リンクや`context_path`の解決はfrontmatterの`id`を軸にする(リネームされてもリンクは壊れない)

## frontmatter

```yaml
---
id: 01J8X...                        # ULID、必須・不変
title: engnoteの設計メモ              # 必須(空文字許容)
tags: [rust, architecture]          # 任意、デフォルト空配列
context_path: /Users/.../engnote    # 任意。特定のディレクトリ/ファイルに紐付ける「開いた世界」ノート用
created: 2026-08-02T15:30:00+09:00  # 必須、自動付与
updated: 2026-08-02T15:30:00+09:00  # 必須、自動更新
---
```

`context_path`がないノートは「閉じた世界」(タグ・バックリンクだけで整理する、Notion的な使い方)、あるノートは「開いた世界」(そのディレクトリに入った時に`engnote here`等で表示される)として扱う。
