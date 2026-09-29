export interface JsonBudget {
  readonly maxBytes: number;
  readonly maxDepth: number;
  readonly maxValues: number;
  readonly maxContainerEntries: number;
  readonly maxStringBytes: number;
  readonly maxTotalStringBytes: number;
}

const encoder = new TextEncoder();

export function utf8Length(value: string): number {
  return encoder.encode(value).byteLength;
}

function boundaryError(label: string, detail: string): never {
  throw new Error(`${label}: ${detail}`);
}

function preflightJsonDepth(source: string, maximum: number, label: string): void {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  for (const character of source) {
    if (quoted) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') quoted = false;
      continue;
    }
    if (character === '"') quoted = true;
    else if (character === "{" || character === "[") {
      depth += 1;
      if (depth > maximum) boundaryError(label, `JSON depth exceeds ${maximum}`);
    } else if (character === "}" || character === "]") {
      depth -= 1;
      if (depth < 0) break; // JSON.parse reports the syntax error.
    }
  }
}

export function assertJsonBudget(value: unknown, budget: JsonBudget, label: string): void {
  const pending: Array<readonly [unknown, number]> = [[value, 0]];
  const seen = new WeakSet<object>();
  let values = 0;
  let stringBytes = 0;
  while (pending.length > 0) {
    const next = pending.pop();
    if (!next) break;
    const [current, parentDepth] = next;
    values += 1;
    if (values > budget.maxValues) boundaryError(label, `JSON value count exceeds ${budget.maxValues}`);
    if (typeof current === "string") {
      const bytes = utf8Length(current);
      if (bytes > budget.maxStringBytes) boundaryError(label, `JSON string exceeds ${budget.maxStringBytes} bytes`);
      stringBytes += bytes;
    } else if (typeof current === "number") {
      if (!Number.isFinite(current)) boundaryError(label, "JSON number is not finite");
    } else if (typeof current === "boolean" || current === null) {
      // JSON scalar.
    } else if (typeof current === "object") {
      if (seen.has(current)) boundaryError(label, "cyclic value is not JSON");
      seen.add(current);
      const depth = parentDepth + 1;
      if (depth > budget.maxDepth) boundaryError(label, `JSON depth exceeds ${budget.maxDepth}`);
      if (Array.isArray(current)) {
        if (current.length > budget.maxContainerEntries) {
          boundaryError(label, `JSON container fanout exceeds ${budget.maxContainerEntries}`);
        }
        for (let index = 0; index < current.length; index += 1) {
          pending.push([current[index], depth]);
        }
      } else {
        if (!isJsonRecord(current)) boundaryError(label, "expected JSON object");
        const prototype = Object.getPrototypeOf(current);
        if (prototype !== Object.prototype && prototype !== null) {
          boundaryError(label, "unsupported JSON object prototype");
        }
        let entryCount = 0;
        for (const key in current) {
          if (!Object.prototype.hasOwnProperty.call(current, key)) continue;
          entryCount += 1;
          if (entryCount > budget.maxContainerEntries) {
            boundaryError(label, `JSON container fanout exceeds ${budget.maxContainerEntries}`);
          }
          const bytes = utf8Length(key);
          if (bytes > budget.maxStringBytes) boundaryError(label, `JSON key exceeds ${budget.maxStringBytes} bytes`);
          stringBytes += bytes;
          pending.push([current[key], depth]);
        }
      }
    } else {
      boundaryError(label, `unsupported JSON value type ${typeof current}`);
    }
    if (stringBytes > budget.maxTotalStringBytes) {
      boundaryError(label, `aggregate JSON strings exceed ${budget.maxTotalStringBytes} bytes`);
    }
  }
}

export function assertSerializedJsonBytes(value: unknown, budget: JsonBudget, label: string): void {
  assertJsonBudget(value, budget, label);
  let serialized: string | undefined;
  try {
    serialized = JSON.stringify(value);
  } catch (error) {
    boundaryError(label, `cannot serialize JSON: ${String(error)}`);
  }
  if (serialized === undefined) boundaryError(label, "value has no JSON representation");
  if (utf8Length(serialized) > budget.maxBytes) boundaryError(label, `JSON exceeds ${budget.maxBytes} bytes`);
}

export function parseBoundedJson(source: string, budget: JsonBudget, label: string): unknown {
  if (utf8Length(source) > budget.maxBytes) boundaryError(label, `JSON exceeds ${budget.maxBytes} bytes`);
  preflightJsonDepth(source, budget.maxDepth, label);
  let value: unknown;
  try {
    value = JSON.parse(source);
  } catch (error) {
    boundaryError(label, `invalid JSON: ${String(error)}`);
  }
  assertJsonBudget(value, budget, label);
  return value;
}

export async function readBoundedJsonResponse(
  response: Response,
  budget: JsonBudget,
  label: string,
): Promise<unknown> {
  if (!response.headers.get("content-type")?.toLowerCase().startsWith("application/json")) {
    boundaryError(label, `HTTP ${response.status}: expected JSON`);
  }
  const length = response.headers.get("content-length");
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > budget.maxBytes)) {
    boundaryError(label, "oversized or invalid Content-Length");
  }
  if (!response.body) boundaryError(label, "missing response body");
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > budget.maxBytes) boundaryError(label, "oversized response");
      chunks.push(value);
    }
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let source: string;
  try {
    source = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch (error) {
    boundaryError(label, `invalid UTF-8 JSON: ${String(error)}`);
  }
  return parseBoundedJson(source, budget, label);
}

export function isJsonRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function requireJsonRecord(value: unknown, label: string): Record<string, unknown> {
  if (!isJsonRecord(value)) boundaryError(label, "expected object");
  return value;
}

export function requireExactKeys(
  value: Record<string, unknown>,
  required: readonly string[],
  optional: readonly string[] = [],
  label = "JSON object",
): void {
  const present = new Set(Object.keys(value));
  const allowed = new Set([...required, ...optional]);
  const missing = required.find((key) => !present.has(key));
  const unknown = Object.keys(value).find((key) => !allowed.has(key));
  if (missing) boundaryError(label, `missing field ${missing}`);
  if (unknown) boundaryError(label, `unknown field ${unknown}`);
}
