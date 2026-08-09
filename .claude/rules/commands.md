# Commands

- Build: `cargo build`
- Run the CLI: `cargo run -p engnote-cli` (binary name: `engnote`)
- Check: `cargo check`
- Test all: `cargo test`
- Test a single crate: `cargo test -p engnote-core`

## GUI (`engnote-gui`) — reinstall after every change

The user verifies GUI changes by clicking the Dock icon, not `cargo run`. After
any edit to `crates/engnote-gui`, rebuild and reinstall `/Applications/engnote.app`
so that's up to date, without being asked each time:

```bash
cd crates/engnote-gui
cargo build --release -p engnote-gui   # fail fast on compile errors first
rm -rf ../../target/release/bundle
cargo bundle --release
rm -rf /Applications/engnote.app
cp -R ../../target/release/bundle/osx/engnote.app /Applications/
codesign --force --deep --sign - /Applications/engnote.app
```

The `codesign` step is required — see README's GUI section for why (without it,
the Dock icon falls back to a generic placeholder once the app is running).
