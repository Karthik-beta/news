// Pure pieces of the ClickHouse access setup (used by clickhouse-setup.ts, covered by scripts/tests).
import { run } from "./lib";

export const PASSWORD_PLACEHOLDER = "{{NEWS_CH_PASSWORD}}";

/**
 * Why this password cannot be used in the access SQL, or undefined when it is fine. It goes inside a
 * single-quoted ClickHouse string literal, where only `'` and `\` are special (`$` is not), so those two are
 * refused rather than escaped: the password in .env and the one ClickHouse stores must be the same text.
 */
export function passwordProblem(password: string): string | undefined {
  if (!password) return "NEWS_CH_PASSWORD is empty. Run `bun run setup` first.";
  if (/['\\]/.test(password)) return "NEWS_CH_PASSWORD must not contain quotes or backslashes.";
  return undefined;
}

/** The access SQL with the password filled in. The result holds a secret: never log it or pass it as an argument. */
export function renderAccessSql(template: string, password: string): string {
  // A function replacer, so `$&`, `$$` and friends in the password are inserted literally.
  return template.replaceAll(PASSWORD_PLACEHOLDER, () => password);
}

/** `docker` arguments that open a clickhouse-client in the container reading SQL from stdin (`-i`). */
export function dockerExecArgs(container: string): string[] {
  return ["exec", "-i", container, "clickhouse-client", "--multiquery"];
}

/** Run the rendered SQL in the container. The SQL (and so the password) goes on stdin, never in argv. */
export function applyAccess(container: string, sql: string, runner: typeof run = run) {
  return runner("docker", dockerExecArgs(container), { input: sql });
}
