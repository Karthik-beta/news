# news-service

The Bureau service host ([ADR 0007](../../../docs/adr/0007-bureau-service-and-board.md), [ADR 0008](../../../docs/adr/0008-chief-in-the-bureau-service.md) §9): startup, the singleton lock and its fence, recovery, the pools, Chief's and Rota's ticks, and the Hotline's command handlers.

## Install it (you run this, in your own terminal)

```
news service install --dry-run   print the plan; changes nothing
news service install             register the service under your account
news service start | stop | status
news service uninstall           stop and remove the service and Board's log-on entry
```

`install`:

1. Re-launches `news.exe` elevated if the terminal is not (Windows asks permission; creating a service needs an administrator token). The elevated window asks for your Windows password at a prompt that does not echo. Only you type it; it goes to the Service Control Manager and nowhere else, and the buffer is overwritten afterwards. Use the account password, not the Windows Hello PIN.
2. Grants your account the "Log on as a service" right (`CreateService` does not).
3. Creates `NewsBureau` (or updates it if it exists, which is how you change the stored password after changing your Windows password): runs `news.exe service run` as `.\<you>`, delayed automatic start, restart after 5 s, then 60 s, then 60 s, also after a deliberate non-zero exit (the update restart).
4. Lets your account query, start and stop the service without elevation, so Board's "Start service" works.
5. Adds Board's log-on entry (`board --minimized`).

If the first start fails with error 1069 or 1326 and passwordless sign-in is on, turn off "only allow Windows Hello sign-in for Microsoft accounts" (Settings, Accounts, Sign-in options), run `install` again, and turn it back on. The service must run as your account because `claude -p` uses your Claude login and Docker runs under your user.

`news service run` is the SCM entry point; from a terminal it falls back to console mode (`--console` forces it), Ctrl-C stops it.

## Updates (`service.update`, from Board)

The new build is staged next to the installed one as `news.exe.new` (the build step copies it; the service downloads nothing). From Board, `service.update` stops new `ai` and `publish` claims, waits until no AI call or publish is in flight, renames `news.exe` to `news.exe.old` and the staged file into place, and exits with code 75, which the SCM's failure actions turn into a start of the new build. A run keeps its steps in PostgreSQL, so Chief's recovery continues it at the next unfinished step. Board compares `service.ping`'s `build` with its own and asks for a restart before it sends another H-tier command.

## Docker (`docker.status|start|stop`)

Through the `docker` command line (a `Runner` trait, faked in tests). Only `news-postgres`, `news-otel` and `news-dynamodb` can be started and stopped; the shared ClickHouse is health-checked only; any other name never reaches a command line.

## Modules

`host` (the loop), `handlers` (the Hotline's commands), `reads` (R commands, shared with the terminal), `log` (the 2,000-line ring), `docker`, `update`, `install` (the plan and the Windows administration; the Win32 calls are in `news-win`).
