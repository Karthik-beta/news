// A minimal in-memory R2 bucket for unit tests: put, get (with ranges), head, delete. Integration tests use
// Miniflare's real local R2 instead.

interface Stored {
  bytes: Uint8Array;
  contentType?: string;
  cacheControl?: string;
  customMetadata?: Record<string, string>;
}

export class FakeBucket {
  readonly objects = new Map<string, Stored>();
  readonly writes: { op: "put" | "delete"; key: string }[] = [];

  async put(
    key: string,
    value: string,
    options?: {
      httpMetadata?: { contentType?: string; cacheControl?: string };
      customMetadata?: Record<string, string>;
    },
  ) {
    this.writes.push({ op: "put", key });
    this.objects.set(key, {
      bytes: new TextEncoder().encode(value),
      contentType: options?.httpMetadata?.contentType,
      cacheControl: options?.httpMetadata?.cacheControl,
      ...(options?.customMetadata ? { customMetadata: { ...options.customMetadata } } : {}),
    });
    return { key };
  }

  async delete(keys: string | string[]) {
    for (const key of Array.isArray(keys) ? keys : [keys]) {
      this.writes.push({ op: "delete", key });
      this.objects.delete(key);
    }
  }

  async head(key: string) {
    const o = this.objects.get(key);
    return o ? { key, size: o.bytes.length, httpMetadata: { contentType: o.contentType } } : null;
  }

  async get(key: string, options?: { range?: { offset?: number; length?: number } }) {
    const o = this.objects.get(key);
    if (!o) return null;
    const offset = options?.range?.offset ?? 0;
    const length = options?.range?.length ?? o.bytes.length;
    const slice = o.bytes.slice(offset, offset + length);
    return {
      key,
      size: o.bytes.length,
      httpMetadata: { contentType: o.contentType, cacheControl: o.cacheControl },
      customMetadata: o.customMetadata ?? {},
      arrayBuffer: async () => slice.buffer.slice(slice.byteOffset, slice.byteOffset + slice.byteLength),
      text: async () => new TextDecoder().decode(slice),
    };
  }

  text(key: string): string | undefined {
    const o = this.objects.get(key);
    return o && new TextDecoder().decode(o.bytes);
  }

  asBucket(): R2Bucket {
    return this as unknown as R2Bucket;
  }
}
