# Storage model

Markdown files are the source of truth for notes; SQLite is a derived index/cache for search, tags, and links — not the primary store.

Anything that can be recomputed from the Markdown files belongs in the SQLite index, not hand-maintained alongside it.
