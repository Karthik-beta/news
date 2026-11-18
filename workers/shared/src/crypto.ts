// Hashing and secret comparison with Web Crypto only (works in workerd and Node).

const utf8 = new TextEncoder();

export async function sha256Hex(data: string | Uint8Array): Promise<string> {
  const bytes = typeof data === "string" ? utf8.encode(data) : data;
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function sha256Bytes(text: string): Promise<Uint8Array> {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", utf8.encode(text)));
}

/**
 * Compares a presented secret with the expected one in constant time. Both are hashed first, so the comparison
 * always runs over 32 bytes whatever their lengths, and leaks neither the length nor the position of a mismatch.
 */
export async function secretsEqual(presented: string, expected: string): Promise<boolean> {
  const [a, b] = await Promise.all([sha256Bytes(presented), sha256Bytes(expected)]);
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= (a[i] as number) ^ (b[i] as number);
  return diff === 0;
}

/** Random hex string of `bytes` bytes. */
export function randomHex(bytes: number): string {
  const buf = new Uint8Array(bytes);
  crypto.getRandomValues(buf);
  return [...buf].map((b) => b.toString(16).padStart(2, "0")).join("");
}
