// Error responses in the write API's one shape: { "error": { "code", "message", "details"? } }.
// Codes are listed in contracts/cloud/write-api.schema.json#/$defs/error and the OpenAPI file.
import { json } from "@news/worker-shared/http";
import type { Issue } from "@news/worker-shared/validate";

export type ErrorCode =
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "method_not_allowed"
  | "unsupported_content_type"
  | "invalid_json"
  | "invalid_request"
  | "invalid_meta"
  | "invalid_bundle"
  | "invalid_curation"
  | "idempotency_key_required"
  | "idempotency_key_invalid"
  | "idempotency_key_reused"
  | "idempotency_in_progress"
  | "version_conflict"
  | "path_conflict"
  | "unknown_page"
  | "page_withdrawn"
  | "purge_failed"
  | "not_configured"
  | "payload_too_large"
  | "rate_limited"
  | "throttled"
  | "internal";

export const STATUS: Record<ErrorCode, number> = {
  unauthorized: 401,
  forbidden: 403,
  not_found: 404,
  method_not_allowed: 405,
  unsupported_content_type: 400,
  invalid_json: 400,
  invalid_request: 400,
  invalid_meta: 400,
  invalid_bundle: 400,
  invalid_curation: 400,
  idempotency_key_required: 400,
  idempotency_key_invalid: 400,
  idempotency_key_reused: 409,
  idempotency_in_progress: 409,
  version_conflict: 409,
  path_conflict: 409,
  unknown_page: 400,
  page_withdrawn: 409,
  purge_failed: 502,
  not_configured: 500,
  payload_too_large: 413,
  rate_limited: 429,
  throttled: 503,
  internal: 500,
};

export class ApiError extends Error {
  constructor(
    readonly code: ErrorCode,
    message: string,
    readonly details?: Issue[],
    readonly headers: Record<string, string> = {},
  ) {
    super(message);
    this.name = "ApiError";
  }

  get status(): number {
    return STATUS[this.code];
  }

  toResponse(extraHeaders: Record<string, string> = {}): Response {
    const body = {
      error: { code: this.code, message: this.message, ...(this.details?.length ? { details: this.details } : {}) },
    };
    return json(body, this.status, { ...this.headers, ...extraHeaders });
  }
}
