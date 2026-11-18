# news-service

The Bureau service host ([ADR 0007](../../../docs/adr/0007-bureau-service-and-board.md), [ADR 0008](../../../docs/adr/0008-chief-in-the-bureau-service.md) §9): startup, the singleton lock and its fence, recovery, the pools, Chief's and Rota's ticks, and the Hotline's command handlers.

## Install it (you run this, in your own terminal)

```
news service install --dry-run   print the plan; changes nothing
news service install             copy news.exe to Program Files and register the service under your account
news service stage <news.exe>    stage a new build for an update from Board
news service start | stop | status
news service uninstall           stop and remove the service, Board's log-on entry and the install folder
```

`install`, `stage` and `uninstall` re-launch `news.exe` elevated if the terminal is not (Windows asks permission). Run them from the build you want, for example `cli\target\release\news.exe service install`.

`install`:

1. Checks that the account (`.\<you>`, or `--account`) resolves to your own SID (`S-1-5-21-…`); anything else is refused before an access list is touched.
2. Asks for your Windows password at a prompt that does not echo. Only you type it. It is read with `ReadConsoleW` into one buffer allocated at full size, passed from that buffer to the Service Control Manager with `ChangeServiceConfigW`, and zeroed when it is dropped; it is never in a Rust `String`, the standard input's buffer, or the `windows-service` crate. (The console host's input buffer and the SCM are Windows'.) Use the account password, not the Windows Hello PIN.
3. Copies `news.exe` to `%ProgramFiles%\News\news.exe` and sets the folder's access list: owner Administrators; SYSTEM and Administrators full control; Users read and execute; nothing inherited. A build tool or agent running as you cannot change the binary the service runs. A build staged before this install is deleted.
4. Grants your account the "Log on as a service" right (`CreateService` does not).
5. Creates `NewsBureau` (or updates it if it exists, which is how you change the stored password after changing your Windows password): runs `%ProgramFiles%\News\news.exe service run` as `.\<you>`, delayed automatic start, restart after 5 s, then 60 s, then 60 s, also after a deliberate non-zero exit (the update restart). The service keeps only `SeChangeNotifyPrivilege` (`SERVICE_CONFIG_REQUIRED_PRIVILEGES_INFO`); every other privilege is removed from its token and from the processes it starts.
6. Lets your account query, start and stop the service without elevation (one access-list entry for your SID, added with `SetEntriesInAclW`), so Board's "Start service" works.
7. Adds Board's log-on entry for the installed binary (`"%ProgramFiles%\News\news.exe" board --minimized`).

Board and `news run status` talk only to a Hotline served by the installed binary, and the service accepts H-tier commands only from Board running from it, so open Board from the installed path (the log-on entry does).

If the first start fails with error 1069 or 1326 and passwordless sign-in is on, turn off "only allow Windows Hello sign-in for Microsoft accounts" (Settings, Accounts, Sign-in options), run `install` again, and turn it back on. The service must run as your account because `claude -p` uses your Claude login and Docker runs under your user. Updating replaces files in Program Files, so your account must be an administrator (the service's log-on token is not filtered by UAC).

`news service run` is the SCM entry point; from a terminal it falls back to console mode (`--console` forces it), Ctrl-C stops it.

## Updates (`news service stage`, then `service.update` from Board)

`news service stage <path>` (elevated) copies the new build to `%ProgramFiles%\News\news.exe.new`, sets the same access list on it, and writes its SHA-256 to `news.exe.new.sha256` beside it; the service downloads nothing. From Board, `service.update` refuses unless the service runs the installed binary and the staged file matches its recorded hash; it then stops new `ai` and `publish` claims, waits until no AI call or publish is in flight, checks the hash again, renames `news.exe` to `news.exe.old` and the staged file into place, deletes the hash file, and exits with code 75, which the SCM's failure actions turn into a start of the new build. A run keeps its steps in PostgreSQL, so Chief's recovery continues it at the next unfinished step. Board compares `service.ping`'s `build` with its own and asks for a restart before it sends another H-tier command.

## Docker (`docker.status|start|stop`)

Through the `docker` command line (a `Runner` trait, faked in tests). Only `news-postgres`, `news-otel` and `news-dynamodb` can be started and stopped; the shared ClickHouse is health-checked only; any other name never reaches a command line.

## Modules

`host` (the loop), `handlers` (the Hotline's commands), `reads` (R commands, shared with the terminal), `log` (the 2,000-line ring), `docker`, `update`, `install` (the plan and the Windows administration; the Win32 calls are in `news-win`).
