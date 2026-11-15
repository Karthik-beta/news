// Vitest globalSetup for integration suites that need DynamoDB Local. When it is down, the whole run stops with one
// plain line (no stack trace) saying what to do.
import { requireDynamoDb } from "./service-check";

export default async function setup(): Promise<void> {
  try {
    await requireDynamoDb();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
