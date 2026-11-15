import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  composeEnv,
  expandedKeys,
  getEnvValue,
  newSecret,
  noEnvFileProblem,
  parseEnv,
  reloadChildEnv,
  setEnvValue,
  withLocalSecrets,
} from "../env-file";
import { root } from "../lib";
import { makeScratchRepo, removeDir, runBun, sandboxEnv } from "./helpers";

const example = readFileSync(join(root, ".env.example"), "utf8");
const counter = () => {
  let n = 0;
  return () => `secret${++n}`;
};

describe("getEnvValue", () => {
  test("reads a value, trims it, and returns '' for empty or missing keys", () => {
    const text = "A=1\nB=  spaced  \nC=\n";
    expect(getEnvValue(text, "A")).toBe("1");
    expect(getEnvValue(text, "B")).toBe("spaced");
    expect(getEnvValue(text, "C")).toBe("");
    expect(getEnvValue(text, "MISSING")).toBe("");
  });

  test("ignores commented-out lines and keys that only end with the name", () => {
    expect(getEnvValue("# NEWS_PG_PASSWORD=hidden\n", "NEWS_PG_PASSWORD")).toBe("");
    expect(getEnvValue("X_NEWS_PG_PASSWORD=hidden\n", "NEWS_PG_PASSWORD")).toBe("");
  });

  test("keeps '=' inside values and the first duplicate wins", () => {
    expect(getEnvValue("URL=postgres://u:p@h/db?a=b\n", "URL")).toBe("postgres://u:p@h/db?a=b");
    expect(getEnvValue("K=first\nK=second\n", "K")).toBe("first");
  });

  test("strips one pair of matching surrounding quotes, like Compose and Bun", () => {
    expect(getEnvValue('P="keep-pg"\n', "P")).toBe("keep-pg");
    expect(getEnvValue("P='keep-ch'\n", "P")).toBe("keep-ch");
    expect(getEnvValue('P=""\n', "P")).toBe("");
    expect(getEnvValue("P=\"mismatched'\n", "P")).toBe("\"mismatched'");
    expect(getEnvValue('P=a"b"\n', "P")).toBe('a"b"');
  });

  test("a quoted password reaches DATABASE_URL without its quotes", () => {
    const out = withLocalSecrets('NEWS_PG_PASSWORD="keep-pg"\nNEWS_CH_PASSWORD=x\nDATABASE_URL=\n');
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:keep-pg@127.0.0.1:5433/news");
  });

  test("handles Windows line endings without leaking a carriage return", () => {
    expect(getEnvValue("A=1\r\nB=2\r\n", "A")).toBe("1");
    expect(getEnvValue("A=1\r\nB=2\r\n", "B")).toBe("2");
  });
});

describe("setEnvValue", () => {
  test("replaces an existing line in place and leaves the others alone", () => {
    expect(setEnvValue("A=1\nB=2\nC=3\n", "B", "9")).toBe("A=1\nB=9\nC=3\n");
  });

  test("fills an empty value", () => {
    expect(setEnvValue("A=\n", "A", "x")).toBe("A=x\n");
  });

  test("appends a missing key, with or without a trailing newline", () => {
    expect(setEnvValue("A=1\n", "B", "2")).toBe("A=1\nB=2\n");
    expect(setEnvValue("A=1", "B", "2")).toBe("A=1\nB=2\n");
    expect(setEnvValue("", "B", "2")).toBe("\nB=2\n");
  });

  test("only the first duplicate is replaced", () => {
    expect(setEnvValue("K=a\nK=b\n", "K", "z")).toBe("K=z\nK=b\n");
  });

  test("does not touch a commented-out line with the same key", () => {
    expect(setEnvValue("# K=old\n", "K", "new")).toBe("# K=old\nK=new\n");
  });

  test("preserves CRLF line endings on the lines around the change", () => {
    expect(setEnvValue("A=1\r\nB=2\r\n", "A", "9")).toBe("A=9\r\nB=2\r\n");
  });

  test("writes `$` patterns in the value literally, both when replacing and when appending", () => {
    for (const value of ["a$&b", "a$$b", "a$1b", "a$`b", "a$'b", "$<x>"]) {
      expect(setEnvValue("A=1\nK=old\nB=2\n", "K", value)).toBe(`A=1\nK=${value}\nB=2\n`);
      expect(setEnvValue("A=1\n", "K", value)).toBe(`A=1\nK=${value}\n`);
    }
  });
});

describe("newSecret", () => {
  test("is URL-safe, long enough to be a password, and different each time", () => {
    const a = newSecret();
    expect(a).toMatch(/^[A-Za-z0-9_-]{32}$/);
    expect(newSecret()).not.toBe(a);
  });
});

describe("withLocalSecrets", () => {
  test("generates both passwords and a DATABASE_URL from .env.example", () => {
    const out = withLocalSecrets(example, counter());
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).toBe("secret1");
    expect(getEnvValue(out, "NEWS_CH_PASSWORD")).toBe("secret2");
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:secret1@127.0.0.1:5433/news");
  });

  test("uses a different generated secret for each password", () => {
    const out = withLocalSecrets(example);
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).not.toBe(getEnvValue(out, "NEWS_CH_PASSWORD"));
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).toHaveLength(32);
  });

  test("never overwrites passwords or a DATABASE_URL that are already set", () => {
    const existing = "NEWS_PG_PASSWORD=keep-pg\nNEWS_CH_PASSWORD=keep-ch\nDATABASE_URL=postgres://custom/db\n";
    const out = withLocalSecrets(existing, () => "SHOULD-NOT-APPEAR");
    expect(out).toBe(existing);
  });

  test("fills only what is missing: an existing PG password is kept and used in the new DATABASE_URL", () => {
    const out = withLocalSecrets("NEWS_PG_PASSWORD=keep-pg\nNEWS_CH_PASSWORD=\nDATABASE_URL=\n", counter());
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).toBe("keep-pg");
    expect(getEnvValue(out, "NEWS_CH_PASSWORD")).toBe("secret1");
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:keep-pg@127.0.0.1:5433/news");
  });

  test("keeps an existing DATABASE_URL even when its password differs from NEWS_PG_PASSWORD", () => {
    const out = withLocalSecrets("NEWS_PG_PASSWORD=a\nNEWS_CH_PASSWORD=b\nDATABASE_URL=postgres://news:other@h/db\n");
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:other@h/db");
  });

  test("honours NEWS_PG_PORT in the generated URL", () => {
    const out = withLocalSecrets("NEWS_PG_PORT=6543\nNEWS_PG_PASSWORD=\nNEWS_CH_PASSWORD=\nDATABASE_URL=\n", counter());
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:secret1@127.0.0.1:6543/news");
  });

  test("is idempotent: a second run changes nothing", () => {
    const once = withLocalSecrets(example, counter());
    expect(withLocalSecrets(once, () => "another")).toBe(once);
  });

  test("adds the keys when the file has none of them (older or hand-written .env)", () => {
    const out = withLocalSecrets("ASTRO_TELEMETRY_DISABLED=1", counter());
    expect(out).toContain("ASTRO_TELEMETRY_DISABLED=1\n");
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).toBe("secret1");
    expect(getEnvValue(out, "DATABASE_URL")).toBe("postgres://news:secret1@127.0.0.1:5433/news");
  });

  test("treats a whitespace-only value as empty and fills it", () => {
    expect(getEnvValue(withLocalSecrets("NEWS_PG_PASSWORD=   \n", counter()), "NEWS_PG_PASSWORD")).toBe("secret1");
  });

  test("does not change any other line of .env.example", () => {
    const out = withLocalSecrets(example, counter());
    const changed = new Set(["NEWS_PG_PASSWORD", "NEWS_CH_PASSWORD", "DATABASE_URL"]);
    const other = (t: string) => t.split("\n").filter((l) => !changed.has(l.split("=")[0] ?? ""));
    expect(other(out)).toEqual(other(example));
  });

  test("a password containing `$&` survives into DATABASE_URL (percent-encoded) and decodes back", () => {
    const out = withLocalSecrets("NEWS_PG_PASSWORD=ab$&cd\nNEWS_CH_PASSWORD=x\nDATABASE_URL=\n");
    const url = getEnvValue(out, "DATABASE_URL");
    expect(url).toBe("postgres://news:ab%24%26cd@127.0.0.1:5433/news");
    expect(decodeURIComponent(new URL(url).password)).toBe("ab$&cd");
  });

  test("URL-special characters in the password are percent-encoded so the URL still parses", () => {
    const pw = "p@ss:w/rd?#%x y$$";
    const out = withLocalSecrets(`NEWS_PG_PASSWORD=${pw}\nNEWS_CH_PASSWORD=x\nDATABASE_URL=\n`);
    const url = new URL(getEnvValue(out, "DATABASE_URL"));
    expect(url.protocol).toBe("postgres:");
    expect(url.username).toBe("news");
    expect(decodeURIComponent(url.password)).toBe(pw);
    expect(url.hostname).toBe("127.0.0.1");
    expect(url.port).toBe("5433");
    expect(url.pathname).toBe("/news");
    // The password itself is left exactly as written.
    expect(getEnvValue(out, "NEWS_PG_PASSWORD")).toBe(pw);
  });
});

describe("bun scripts/setup.ts, end to end in a scratch copy (no docker, no real .env)", () => {
  // The scratch PATH has no tools, so setup writes .env and then fails at its first `docker` step.
  const runSetup = (dir: string) => runBun(["--no-env-file", "scripts/setup.ts"], { cwd: dir, env: sandboxEnv(dir) });

  test("refuses to run when Bun has loaded .env, before writing anything", () => {
    const dir = makeScratchRepo();
    try {
      const r = runBun(["scripts/setup.ts"], { cwd: dir, env: sandboxEnv(dir) });
      expect(r.status).toBe(1);
      expect(r.err).toContain("bun --no-env-file scripts/setup.ts");
      expect(existsSync(join(dir, ".env"))).toBe(false);
    } finally {
      removeDir(dir);
    }
  });

  test("creates .env from .env.example with generated passwords", () => {
    const dir = makeScratchRepo();
    try {
      const r = runSetup(dir);
      const env = readFileSync(join(dir, ".env"), "utf8");
      expect(r.out).toContain("✓ .env ready");
      expect(getEnvValue(env, "NEWS_PG_PASSWORD")).toHaveLength(32);
      expect(getEnvValue(env, "NEWS_CH_PASSWORD")).toHaveLength(32);
      expect(getEnvValue(env, "DATABASE_URL")).toContain(getEnvValue(env, "NEWS_PG_PASSWORD"));
      // Passwords are never printed.
      expect(r.out + r.err).not.toContain(getEnvValue(env, "NEWS_PG_PASSWORD"));
      expect(r.out + r.err).not.toContain(getEnvValue(env, "NEWS_CH_PASSWORD"));
      // It stopped at the docker step, which could not run: nothing real was started.
      expect(r.status).toBe(1);
      expect(r.out + r.err).toContain("start PostgreSQL, OTel Collector and DynamoDB Local");
    } finally {
      removeDir(dir);
    }
  });

  test("re-running never overwrites existing passwords or other settings", () => {
    const existing =
      "NEWS_PG_PASSWORD=my-pg-pw\nNEWS_CH_PASSWORD=my-ch-pw\nDATABASE_URL=postgres://news:my-pg-pw@127.0.0.1:5433/news\nNEWS_PARALLEL=7\n";
    const dir = makeScratchRepo({ ".env": existing });
    try {
      runSetup(dir);
      const first = readFileSync(join(dir, ".env"), "utf8");
      runSetup(dir);
      expect(readFileSync(join(dir, ".env"), "utf8")).toBe(first);
      // Everything that was there is untouched; only the missing DynamoDB Local settings were appended.
      expect(first.startsWith(existing)).toBe(true);
      expect(getEnvValue(first, "NEWS_PARALLEL")).toBe("7");
      expect(getEnvValue(first, "NEWS_DYNAMODB_ENDPOINT")).toBe("http://127.0.0.1:8000");
    } finally {
      removeDir(dir);
    }
  });

  test("copies the DynamoDB Local settings into an older .env, but keeps ones the user already set", () => {
    const dir = makeScratchRepo({
      ".env": "NEWS_PG_PASSWORD=a\nNEWS_CH_PASSWORD=b\nDATABASE_URL=postgres://x\nNEWS_DYNAMODB_PORT=8123\n",
    });
    try {
      runSetup(dir);
      const env = readFileSync(join(dir, ".env"), "utf8");
      expect(getEnvValue(env, "NEWS_DYNAMODB_PORT")).toBe("8123");
      expect(getEnvValue(env, "NEWS_DYNAMODB_ENDPOINT")).toBe("http://127.0.0.1:8123");
      expect(getEnvValue(env, "NEWS_DYNAMODB_REGION")).toBe("us-east-1");
      expect(getEnvValue(env, "NEWS_DYNAMODB_ACCESS_KEY_ID")).toBe("AKIDLOCAL");
      expect(getEnvValue(env, "NEWS_DYNAMODB_SECRET_ACCESS_KEY")).toBe("local");
    } finally {
      removeDir(dir);
    }
  });

  test("fills only the empty values of an existing .env", () => {
    const dir = makeScratchRepo({ ".env": "NEWS_PG_PASSWORD=my-pg-pw\nNEWS_CH_PASSWORD=\nDATABASE_URL=\n" });
    try {
      runSetup(dir);
      const env = readFileSync(join(dir, ".env"), "utf8");
      expect(getEnvValue(env, "NEWS_PG_PASSWORD")).toBe("my-pg-pw");
      expect(getEnvValue(env, "NEWS_CH_PASSWORD")).toHaveLength(32);
      expect(getEnvValue(env, "DATABASE_URL")).toBe("postgres://news:my-pg-pw@127.0.0.1:5433/news");
    } finally {
      removeDir(dir);
    }
  });

  test("the second run of a freshly generated .env keeps the generated values", () => {
    const dir = makeScratchRepo();
    try {
      runSetup(dir);
      const first = readFileSync(join(dir, ".env"), "utf8");
      runSetup(dir);
      expect(readFileSync(join(dir, ".env"), "utf8")).toBe(first);
    } finally {
      removeDir(dir);
    }
  });

  test("does not read the developer's real environment passwords into .env", () => {
    const dir = makeScratchRepo();
    try {
      writeFileSync(join(dir, ".env.example"), "NEWS_PG_PASSWORD=\nNEWS_CH_PASSWORD=\nDATABASE_URL=\n");
      runBun(["--no-env-file", "scripts/setup.ts"], {
        cwd: dir,
        env: sandboxEnv(dir, { NEWS_PG_PASSWORD: "from-the-shell" }),
      });
      // An exported value reaches setup's child processes, but setup builds .env from the file only.
      expect(readFileSync(join(dir, ".env"), "utf8")).not.toContain("from-the-shell");
    } finally {
      removeDir(dir);
    }
  });
});

describe("reloadChildEnv", () => {
  test("fills an empty or unset value from the final .env, DynamoDB included; an exported value wins", () => {
    const env: Record<string, string | undefined> = { NEWS_DYNAMODB_ENDPOINT: "", NEWS_PG_PASSWORD: "exported" };
    reloadChildEnv(
      "NEWS_DYNAMODB_PORT=8123\nNEWS_DYNAMODB_ENDPOINT=http://127.0.0.1:8123\nNEWS_PG_PASSWORD=new\n",
      env,
    );
    expect(env.NEWS_DYNAMODB_ENDPOINT).toBe("http://127.0.0.1:8123");
    expect(env.NEWS_DYNAMODB_PORT).toBe("8123");
    expect(env.NEWS_PG_PASSWORD).toBe("exported");
  });
});

describe("$ in .env values", () => {
  const text = "# comment\nPLAIN=abc\nDOLLAR=ab$cd\nQUOTED='x$Y-z'\nDUP=first\nDUP=second\nEMPTY=\n";

  test("parseEnv reads every key literally, skips comments, first duplicate wins", () => {
    expect(parseEnv(text)).toEqual({
      PLAIN: "abc",
      DOLLAR: "ab$cd",
      QUOTED: "x$Y-z",
      DUP: "first",
      EMPTY: "",
    });
  });

  test("composeEnv passes .env values literally and keeps the shell's other variables", () => {
    // The wrapper runs under `bun --no-env-file`, so `env` is the shell's own environment.
    const env = composeEnv(text, { PATH: "/bin", UNSET: undefined });
    expect(env.PATH).toBe("/bin");
    expect(env.DOLLAR).toBe("ab$cd");
    expect(env.QUOTED).toBe("x$Y-z");
    expect(env.DUP).toBe("first");
    expect("UNSET" in env).toBe(false);
  });

  test("composeEnv: an exported shell variable wins over .env; an empty one does not", () => {
    const env = composeEnv(text, { PLAIN: "from-shell", DOLLAR: "", QUOTED: undefined });
    expect(env.PLAIN).toBe("from-shell");
    expect(env.DOLLAR).toBe("ab$cd");
    expect(env.QUOTED).toBe("x$Y-z");
  });

  test("noEnvFileProblem: the Compose wrapper and setup must run with --no-env-file", () => {
    expect(noEnvFileProblem(["--no-env-file"], "scripts/compose.ts")).toBeUndefined();
    expect(noEnvFileProblem([], "scripts/compose.ts")).toContain("bun --no-env-file scripts/compose.ts");
  });

  test("expandedKeys names only the keys Bun read differently because of a $", () => {
    expect(expandedKeys(text, { PLAIN: "other", DOLLAR: "ab", QUOTED: "x$Y-z" })).toEqual(["DOLLAR"]);
    expect(expandedKeys(text, {})).toEqual([]);
  });
});
