# Database migrations, and the one reset

The database is a single SQLite file. Its shape is recorded in
`schema_meta.version`, and `rbc-db` moves it forward one step at a time
(`migrate_v1` … `migrate_v6`, `SCHEMA_VERSION = 6`). Opening a database runs
every step it is missing, in order; there is no separate "migrate" command.

## The v4 reset

Schema v4 introduced workspaces, boards, studies and the AI tables. The old
single-tab session did not map onto them, so the migration moved the previous
`sessions` rows into `legacy_sessions` and started the new shape empty.

The old rows are **kept for one release**:

```sql
SELECT client, updated FROM legacy_sessions;
```

They are what the reset notice counts, and what a person can recover their old
tabs from by hand. Nothing reads them automatically.

## What a person sees

On load the app calls `GET /api/session/notice`, which answers

```json
{ "reset": true, "legacy_sessions": 1 }
```

when the reset left anything behind. It is shown once, as a banner, guarded by
the localStorage key `rbc.resetNotice.seen`. A fresh database answers
`{"reset": false, "legacy_sessions": 0}` and shows nothing.

## Where the session lives

Two places, on purpose:

- **localStorage** (`rbc.session.v1`) — what the tab restores from first;
- **the `sessions` table** (`GET`/`PUT /api/session?client=…`) — the server copy.

The client id that names that server row is kept in localStorage *and* in an
`rbc_client` cookie. Clearing site data wipes one of them; holding both means
"clear localStorage and reload" still finds the session the server is holding.

## Checking a database

```bash
rusty-brick-chess --help          # the commands
curl -s 127.0.0.1:8090/api/health # {"schema":6,...}
```

`/api/health` reports the version the running binary sees. A number lower than
`SCHEMA_VERSION` in the health output usually means the binary is older than the
checkout — the installer warns about that rather than shipping it.
