# Secrets

No credential belongs in this repository, its history, a ticket, a journal or
a test fixture.

- **Where keys live.** In the sops+age store: `secrets set NAME` (value on
  stdin), `secrets list` for names. Run anything that needs a key as
  `secrets run -- CMD`; the key reaches the process as an environment variable.
- **How the apps hold them.** The AI provider settings hold a *reference*
  (an environment variable name, or `keyring:<service>/<account>`), never the
  key. `apps/rusty-brick-chess/src/providers.rs` (`resolve_secret`) turns the
  reference into a value at call time. New credentials follow the same pattern.
- **What guards the tree.** `.gitignore` refuses key and `.env` files. The
  `secret-scan` CI job and `make check-secrets` run gitleaks over the full
  history with `.gitleaks.toml`. The config allowlists only machine-written
  records (ticket and Warrant journals, generated views, eval fixtures,
  lockfiles); a credential pasted into one of those would not be caught, so do
  not put one there.
- **If a key is committed.** Treat it as leaked: rotate it first, then remove
  it. Rewriting history does not undo the exposure.
