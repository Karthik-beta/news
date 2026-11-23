// The two API surfaces the provisioning script uses, as interfaces (the tests mock them) and thin fetch
// implementations. Credentials come in as arguments, are only ever sent as a header or a signature, and are never
// logged or put in an error message. Nothing here runs unless provision.ts builds a real client in check/apply mode.
import { AwsClient } from "aws4fetch";

export interface Rule {
  ref?: string;
  [key: string]: unknown;
}
export interface Route {
  id?: string;
  pattern: string;
  script?: string;
}
export interface TableInfo {
  provisioned: boolean;
  rcu: number;
  wcu: number;
  keysOk: boolean;
}

export interface Cf {
  bucketExists(name: string): Promise<boolean>;
  createBucket(name: string): Promise<void>;
  customDomains(bucket: string): Promise<string[]>;
  addCustomDomain(bucket: string, domain: string): Promise<void>;
  findD1(name: string): Promise<string | undefined>;
  createD1(name: string): Promise<string>;
  d1Query(id: string, sql: string): Promise<Record<string, unknown>[]>;
  getRules(phase: string): Promise<Rule[]>;
  putRules(phase: string, rules: Rule[]): Promise<void>;
  routes(): Promise<Route[]>;
  setRoute(route: Route): Promise<void>;
}

export interface Aws {
  describeTable(name: string): Promise<TableInfo | undefined>;
  createTable(name: string, rcu: number, wcu: number): Promise<void>;
  setThroughput(name: string, rcu: number, wcu: number): Promise<void>;
  ttlEnabled(name: string, attribute: string): Promise<boolean>;
  enableTtl(name: string, attribute: string): Promise<void>;
  userExists(user: string): Promise<boolean>;
  createUser(user: string): Promise<void>;
  getUserPolicy(user: string, policy: string): Promise<unknown | undefined>;
  putUserPolicy(user: string, policy: string, document: unknown): Promise<void>;
}

const CF = "https://api.cloudflare.com/client/v4";

/** The fields of DynamoDB's answers that are read here. */
interface DynamoJson {
  __type?: string;
  message?: string;
  Table?: {
    TableStatus: string;
    KeySchema: { AttributeName: string; KeyType: string }[];
    BillingModeSummary?: { BillingMode?: string };
    ProvisionedThroughput?: { ReadCapacityUnits?: number; WriteCapacityUnits?: number };
  };
  TimeToLiveDescription?: { TimeToLiveStatus?: string; AttributeName?: string };
}

export function cloudflare(token: string, accountId: string, zoneId: string): Cf {
  async function call(method: string, path: string, body?: unknown, missingOk = false) {
    const res = await fetch(`${CF}${path}`, {
      method,
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (missingOk && res.status === 404) return undefined;
    const json = (await res.json().catch(() => ({}))) as { success?: boolean; result?: unknown; errors?: unknown };
    if (!res.ok || json.success === false) {
      throw new Error(`Cloudflare ${method} ${path} failed: ${res.status} ${JSON.stringify(json.errors ?? [])}`);
    }
    return json.result;
  }
  const acct = `/accounts/${accountId}`;
  const phase = (p: string) => `/zones/${zoneId}/rulesets/phases/${p}/entrypoint`;
  return {
    bucketExists: async (name) => (await call("GET", `${acct}/r2/buckets/${name}`, undefined, true)) !== undefined,
    createBucket: async (name) => void (await call("POST", `${acct}/r2/buckets`, { name })),
    customDomains: async (bucket) => {
      const r = (await call("GET", `${acct}/r2/buckets/${bucket}/domains/custom`)) as {
        domains?: { domain: string }[];
      };
      return (r.domains ?? []).map((d) => d.domain);
    },
    addCustomDomain: async (bucket, domain) =>
      void (await call("POST", `${acct}/r2/buckets/${bucket}/domains/custom`, {
        domain,
        zoneId,
        enabled: true,
        minTLS: "1.2",
      })),
    findD1: async (name) => {
      const r = (await call("GET", `${acct}/d1/database?name=${encodeURIComponent(name)}`)) as {
        uuid: string;
        name: string;
      }[];
      return r.find((d) => d.name === name)?.uuid;
    },
    createD1: async (name) => ((await call("POST", `${acct}/d1/database`, { name })) as { uuid: string }).uuid,
    d1Query: async (id, sql) => {
      const r = (await call("POST", `${acct}/d1/database/${id}/query`, { sql })) as {
        results?: Record<string, unknown>[];
      }[];
      return r.flatMap((s) => s.results ?? []);
    },
    getRules: async (p) =>
      ((await call("GET", phase(p), undefined, true)) as { rules?: Rule[] } | undefined)?.rules ?? [],
    putRules: async (p, rules) => void (await call("PUT", phase(p), { rules })),
    routes: async () => (await call("GET", `/zones/${zoneId}/workers/routes`)) as Route[],
    setRoute: async (route) => {
      const { id, ...body } = route;
      await call(id ? "PUT" : "POST", `/zones/${zoneId}/workers/routes${id ? `/${id}` : ""}`, body);
    },
  };
}

export function aws(
  creds: { accessKeyId: string; secretAccessKey: string; sessionToken?: string },
  region: string,
  sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
): Aws {
  const dynamoClient = new AwsClient({ ...creds, region, service: "dynamodb" });
  const iamClient = new AwsClient({ ...creds, region: "us-east-1", service: "iam" });

  async function dynamo(action: string, body: unknown, missingOk = false) {
    const res = await dynamoClient.fetch(`https://dynamodb.${region}.amazonaws.com/`, {
      method: "POST",
      headers: { "content-type": "application/x-amz-json-1.0", "x-amz-target": `DynamoDB_20120810.${action}` },
      body: JSON.stringify(body),
    });
    const json = (await res.json().catch(() => ({}))) as DynamoJson;
    if (missingOk && String(json.__type).endsWith("ResourceNotFoundException")) return undefined;
    if (!res.ok) throw new Error(`DynamoDB ${action} failed: ${res.status} ${json.__type ?? ""} ${json.message ?? ""}`);
    return json;
  }
  // IAM answers in XML: the few fields read here are pulled out with a pattern.
  async function iam(action: string, params: Record<string, string>, missingOk = false) {
    const res = await iamClient.fetch("https://iam.amazonaws.com/", {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ Action: action, Version: "2010-05-08", ...params }),
    });
    const text = await res.text();
    if (missingOk && text.includes("<Code>NoSuchEntity</Code>")) return undefined;
    if (!res.ok) throw new Error(`IAM ${action} failed: ${res.status} ${text.match(/<Code>(.*?)<\/Code>/)?.[1] ?? ""}`);
    return text;
  }
  const throughput = (rcu: number, wcu: number) => ({ ReadCapacityUnits: rcu, WriteCapacityUnits: wcu });
  async function active(name: string) {
    for (let i = 0; i < 30; i++) {
      const t = (await dynamo("DescribeTable", { TableName: name })) as DynamoJson;
      if (t.Table?.TableStatus === "ACTIVE") return;
      await sleep(2000);
    }
    throw new Error(`Table ${name} is not ACTIVE after 60 seconds; run the script again.`);
  }
  return {
    describeTable: async (name) => {
      const r = await dynamo("DescribeTable", { TableName: name }, true);
      if (!r) return undefined;
      const t = r.Table as NonNullable<DynamoJson["Table"]>;
      const keys = t.KeySchema.map((k) => `${k.AttributeName}:${k.KeyType}`);
      return {
        provisioned: t.BillingModeSummary?.BillingMode !== "PAY_PER_REQUEST",
        rcu: t.ProvisionedThroughput?.ReadCapacityUnits ?? 0,
        wcu: t.ProvisionedThroughput?.WriteCapacityUnits ?? 0,
        keysOk: keys.length === 2 && keys[0] === "PK:HASH" && keys[1] === "SK:RANGE",
      };
    },
    createTable: async (name, rcu, wcu) => {
      await dynamo("CreateTable", {
        TableName: name,
        BillingMode: "PROVISIONED",
        ProvisionedThroughput: throughput(rcu, wcu),
        AttributeDefinitions: [
          { AttributeName: "PK", AttributeType: "S" },
          { AttributeName: "SK", AttributeType: "S" },
        ],
        KeySchema: [
          { AttributeName: "PK", KeyType: "HASH" },
          { AttributeName: "SK", KeyType: "RANGE" },
        ],
      });
      await active(name);
    },
    setThroughput: async (name, rcu, wcu) => {
      await dynamo("UpdateTable", {
        TableName: name,
        BillingMode: "PROVISIONED",
        ProvisionedThroughput: throughput(rcu, wcu),
      });
      await active(name);
    },
    ttlEnabled: async (name, attribute) => {
      const r = (await dynamo("DescribeTimeToLive", { TableName: name })) as DynamoJson;
      const d = r.TimeToLiveDescription ?? {};
      return (d.TimeToLiveStatus === "ENABLED" || d.TimeToLiveStatus === "ENABLING") && d.AttributeName === attribute;
    },
    enableTtl: async (name, attribute) =>
      void (await dynamo("UpdateTimeToLive", {
        TableName: name,
        TimeToLiveSpecification: { Enabled: true, AttributeName: attribute },
      })),
    userExists: async (user) => (await iam("GetUser", { UserName: user }, true)) !== undefined,
    createUser: async (user) => void (await iam("CreateUser", { UserName: user })),
    getUserPolicy: async (user, policy) => {
      const text = await iam("GetUserPolicy", { UserName: user, PolicyName: policy }, true);
      const doc = text?.match(/<PolicyDocument>(.*?)<\/PolicyDocument>/s)?.[1];
      return doc === undefined ? undefined : JSON.parse(decodeURIComponent(doc));
    },
    putUserPolicy: async (user, policy, document) =>
      void (await iam("PutUserPolicy", {
        UserName: user,
        PolicyName: policy,
        PolicyDocument: JSON.stringify(document),
      })),
  };
}
