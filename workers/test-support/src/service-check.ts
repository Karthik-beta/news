// Node-side fail-fast check for the local services the integration tests need.
import { dynamoConfigFromEnv } from "./dynamodb";

export const INFRA_HINT = "Run `bun run infra:up` first.";

/**
 * Throws a one-line error when DynamoDB Local is not answering at the configured endpoint.
 * An unsigned GET / is answered with a DynamoDB-shaped error (`com.amazonaws.dynamodb...#MissingAuthenticationToken`),
 * older builds with a "healthy: dynamodb..." line. Anything else on that port is not DynamoDB Local.
 */
export async function requireDynamoDb(env: Record<string, unknown> = process.env): Promise<void> {
  const { endpoint } = dynamoConfigFromEnv(env);
  let body: string;
  try {
    const res = await fetch(endpoint, { signal: AbortSignal.timeout(3000) });
    body = await res.text();
  } catch {
    throw new Error(`DynamoDB Local is not reachable at ${endpoint}. ${INFRA_HINT}`);
  }
  if (!/com\.amazonaws\.dynamodb|healthy:\s*dynamodb/i.test(body)) {
    throw new Error(`Something answers at ${endpoint} but it is not DynamoDB Local. ${INFRA_HINT}`);
  }
}
