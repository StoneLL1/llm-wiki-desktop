import type { TFunction } from "i18next";

export type BackendErrorActionKind =
  | "retry"
  | "reauthorize"
  | "repair"
  | "open_settings"
  | "restart"
  | "copy_details"
  | null;

export interface NormalizedBackendError {
  code: string | null;
  summaryKey: string;
  summaryParams?: Record<string, string | number>;
  technicalDetails: string | null;
  recoverable: boolean;
  userActionRequired: boolean;
  actionKind: BackendErrorActionKind;
}

export interface NormalizeBackendErrorOptions {
  defaultSummaryKey?: string;
  defaultSummaryParams?: Record<string, string | number>;
  defaultRecoverable?: boolean;
  defaultUserActionRequired?: boolean;
  defaultActionKind?: BackendErrorActionKind;
  actionKindOverride?: BackendErrorActionKind;
}

const REDACTED = "[REDACTED]";
const REDACTED_PATH = "[REDACTED PATH]";
const MAX_DETAILS_LENGTH = 12_000;
const MAX_OBJECT_DEPTH = 8;
const SENSITIVE_KEY = /(?:authorization|cookie|password|passwd|secret|token|api[_-]?key|credential|session[_-]?key|private[_-]?key)/i;
const PATH_KEY = /(?:path|root|directory|folder|file)/i;
const ACTION_KINDS: BackendErrorActionKind[] = [
  "retry",
  "reauthorize",
  "repair",
  "open_settings",
  "restart",
  "copy_details",
  null,
];

interface ErrorPresentation {
  summaryKey: string;
  actionKind?: BackendErrorActionKind;
}

const OPEN_SETTINGS = "open_settings";
const RETRY = "retry";
const PROVIDER_SUMMARY = "backendError.summary.provider";

const EXACT_ACTIONS: Record<string, BackendErrorActionKind> = {
  AGENT_UNAVAILABLE: OPEN_SETTINGS,
  IMPORT_V2_CAPABILITY_INVALID: OPEN_SETTINGS,
  IMPORT_V2_CAPABILITY_UNAVAILABLE: OPEN_SETTINGS,
  LLM_AUTH_FAILED: "reauthorize",
  LLM_SECRET_MISSING: "reauthorize",
  LLM_BASE_URL_SECRET_FORBIDDEN: OPEN_SETTINGS,
  LLM_PROVIDER_MISSING: OPEN_SETTINGS,
  LLM_BASE_URL_INVALID: OPEN_SETTINGS,
  LLM_MODEL_REQUIRED: OPEN_SETTINGS,
  LLM_RATE_LIMITED: RETRY,
  LLM_REQUEST_TIMEOUT: RETRY,
  LLM_REQUEST_FAILED: RETRY,
  LLM_CLIENT_FAILED: RETRY,
  LLM_PROVIDER_ERROR: RETRY,
  LLM_RESPONSE_INVALID: RETRY,
  LLM_CANCELLED: null,
  PROJECT_REPAIR_REQUIRED: "repair",
  UPDATE_RESTART_REQUIRED: "restart",
  UPDATER_RESTART_REQUIRED: "restart",
};

const EXACT_SUMMARIES: Record<string, string> = {
  AGENT_UNAVAILABLE: "backendError.summary.chat",
  IMPORT_V2_CAPABILITY_INVALID: "backendError.summary.importCapabilityInvalid",
  IMPORT_V2_CAPABILITY_UNAVAILABLE: "backendError.summary.importCapabilityUnavailable",
  PROJECT_REPAIR_REQUIRED: "backendError.summary.projectRepairRequired",
  UPDATE_RESTART_REQUIRED: "backendError.summary.restartRequired",
  UPDATER_RESTART_REQUIRED: "backendError.summary.restartRequired",
};

const PREFIX_PRESENTATIONS: Array<[RegExp, ErrorPresentation]> = [
  [/(?:UNTRUSTED|READ_ONLY|AUTHORITY|PERMIT|POLICY|FORBIDDEN|DENIED|OUTSIDE_PROJECT)/, {
    summaryKey: "backendError.summary.securityRestriction",
    actionKind: OPEN_SETTINGS,
  }],
  [/^(?:PATH_|PROJECT_)/, {
    summaryKey: "backendError.summary.project",
  }],
  [/^(?:IMPORT_|EXTRACT_|CAPABILITY_)/, {
    summaryKey: "backendError.summary.import",
  }],
  [/^(?:LLM_|PROVIDER_|SECRET_)/, {
    summaryKey: PROVIDER_SUMMARY,
  }],
  [/^(?:UPDATE_|UPDATER_)/, {
    summaryKey: "backendError.summary.update",
  }],
  [/^(?:TASK_|WORKFLOW_)/, {
    summaryKey: "backendError.summary.task",
  }],
  [/^CHAT_/, {
    summaryKey: "backendError.summary.chat",
  }],
];

function safeProperty(value: object, key: string): unknown {
  try {
    return (value as Record<string, unknown>)[key];
  } catch {
    return undefined;
  }
}

function stringProperty(value: object, key: string): string | null {
  const candidate = safeProperty(value, key);
  return typeof candidate === "string" && candidate.trim() ? candidate.trim() : null;
}

function booleanProperty(value: object, key: string): boolean | undefined {
  const candidate = safeProperty(value, key);
  return typeof candidate === "boolean" ? candidate : undefined;
}

function isError(value: object): value is Error {
  try {
    return value instanceof Error;
  } catch {
    return false;
  }
}

function isArray(value: object): value is unknown[] {
  try {
    return Array.isArray(value);
  } catch {
    return false;
  }
}

function redactUrlQueries(value: string): string {
  return value.replace(/https?:\/\/[^\s<>"']+/giu, (url) => url.replace(
    /([?&][^=&#\s]+)=([^&#\s]*)/gu,
    `$1=${REDACTED}`,
  ));
}

function redactAbsolutePaths(value: string): string {
  return value
    .replace(/\bfile:\/{2,3}[^"'\r\n]*/giu, REDACTED_PATH)
    .replace(/(^|[^A-Za-z0-9\\])\\\\[^\\\r\n]+\\[^"'\r\n]*/gu, (_match, prefix: string) => (
      `${prefix}${REDACTED_PATH}`
    ))
    .replace(/(^|[^A-Za-z0-9])[A-Za-z]:[\\/][^"'\r\n]*/gu, (_match, prefix: string) => (
      `${prefix}${REDACTED_PATH}`
    ))
    .replace(/(^|[^A-Za-z0-9/])\/(?!\/)[^"'\r\n]*/gu, (_match, prefix: string) => (
      `${prefix}${REDACTED_PATH}`
    ));
}

function redactString(value: string): string {
  let next = value;
  next = next.replace(
    /(authorization'?\s*[:=]\s*')[^']*'/giu,
    `$1${REDACTED}'`,
  );
  next = next.replace(
    /(authorization"?\s*[:=]\s*")[^"]*"/giu,
    `$1${REDACTED}"`,
  );
  next = next.replace(
    /(authorization"?\s*[:=]\s*)(?:bearer\s+|basic\s+)?[^"'\r\n]+/giu,
    `$1${REDACTED}`,
  );
  next = next.replace(/((?:set-)?cookie'?\s*[:=]\s*')[^']*'/giu, `$1${REDACTED}'`);
  next = next.replace(/((?:set-)?cookie"?\s*[:=]\s*"?)[^"\r\n]*/giu, `$1${REDACTED}`);
  next = next.replace(
    /((?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|passwd|secret|token)'?\s*[:=]\s*')[^']*'/giu,
    `$1${REDACTED}'`,
  );
  next = next.replace(
    /((?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|passwd|secret|token)"?\s*[:=]\s*")[^"]*"/giu,
    `$1${REDACTED}"`,
  );
  next = next.replace(
    /((?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|password|passwd|secret|token)"?\s*[:=]\s*)[^"'\r\n]+/giu,
    `$1${REDACTED}`,
  );
  next = redactUrlQueries(next);
  return redactAbsolutePaths(next);
}

function sanitizedValue(
  value: unknown,
  seen: WeakSet<object>,
  depth: number,
  key: string | null = null,
): unknown {
  if (key && SENSITIVE_KEY.test(key)) return REDACTED;
  if (value === null || value === undefined) return value ?? null;
  if (typeof value === "string") {
    if (key && PATH_KEY.test(key) && /^(?:[A-Za-z]:[\\/]|\\\\|\/)/u.test(value)) {
      return REDACTED_PATH;
    }
    return redactString(value);
  }
  if (typeof value === "number" || typeof value === "boolean") return value;
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "symbol") return value.description ? `[Symbol ${value.description}]` : "[Symbol]";
  if (typeof value === "function") return "[Function]";
  if (depth >= MAX_OBJECT_DEPTH) return "[Truncated]";
  if (typeof value !== "object") return redactString(`${value}`);
  if (seen.has(value)) return "[Circular]";
  seen.add(value);

  if (isError(value)) {
    const name = stringProperty(value, "name") ?? "Error";
    const message = stringProperty(value, "message") ?? "";
    const stack = stringProperty(value, "stack");
    const normalized = {
      name: redactString(name),
      message: redactString(message),
      stack: stack ? redactString(stack) : null,
    };
    seen.delete(value);
    return normalized;
  }

  if (isArray(value)) {
    const normalized = value.slice(0, 50).map((item) => sanitizedValue(item, seen, depth + 1));
    if (value.length > 50) normalized.push(`[${value.length - 50} more items]`);
    seen.delete(value);
    return normalized;
  }

  const normalized: Record<string, unknown> = {};
  let keys: string[] = [];
  try {
    keys = Object.keys(value).slice(0, 100);
  } catch {
    seen.delete(value);
    return "[Uninspectable object]";
  }
  for (const property of keys) {
    normalized[property] = sanitizedValue(safeProperty(value, property), seen, depth + 1, property);
  }
  seen.delete(value);
  return normalized;
}

function serializeTechnicalDetails(value: unknown): string | null {
  if (value === undefined) return null;
  if (typeof value === "string") return redactString(value).slice(0, MAX_DETAILS_LENGTH) || null;
  try {
    const serialized = JSON.stringify(sanitizedValue(value, new WeakSet(), 0), null, 2);
    if (!serialized) return null;
    return redactString(serialized).slice(0, MAX_DETAILS_LENGTH);
  } catch {
    return "[Unserializable error details]";
  }
}

function isNormalizedBackendError(value: unknown): value is NormalizedBackendError {
  if (typeof value !== "object" || value === null) return false;
  const technicalDetails = safeProperty(value, "technicalDetails");
  const actionKind = safeProperty(value, "actionKind");
  return typeof safeProperty(value, "summaryKey") === "string"
    && (technicalDetails === null || typeof technicalDetails === "string")
    && typeof safeProperty(value, "recoverable") === "boolean"
    && typeof safeProperty(value, "userActionRequired") === "boolean"
    && ACTION_KINDS.includes(actionKind as BackendErrorActionKind);
}

function presentationForCode(code: string | null): ErrorPresentation | null {
  if (!code) return null;
  const prefix = PREFIX_PRESENTATIONS.find(([pattern]) => pattern.test(code))?.[1] ?? null;
  const summaryKey = Object.hasOwn(EXACT_SUMMARIES, code)
    ? EXACT_SUMMARIES[code]
    : prefix?.summaryKey;
  if (!summaryKey) return null;
  return {
    summaryKey,
    actionKind: Object.hasOwn(EXACT_ACTIONS, code) ? EXACT_ACTIONS[code] : prefix?.actionKind,
  };
}

function defaultAction(
  recoverable: boolean,
  userActionRequired: boolean,
  hasDetails: boolean,
): BackendErrorActionKind {
  if (userActionRequired) return "open_settings";
  if (recoverable) return "retry";
  return hasDetails ? "copy_details" : null;
}

export function redactBackendErrorDetails(value: string): string {
  return redactString(value).slice(0, MAX_DETAILS_LENGTH);
}

export function backendErrorCode(error: unknown): string | null {
  if (typeof error !== "object" || error === null) return null;
  return stringProperty(error, "code");
}

export function isAiConfigurationErrorCode(code: string | null): boolean {
  return Boolean(code && /^(?:AGENT_|LLM_|PROVIDER_|SECRET_)/u.test(code));
}

export function normalizeBackendError(
  error: unknown,
  options: NormalizeBackendErrorOptions = {},
): NormalizedBackendError {
  if (isNormalizedBackendError(error)) {
    const technicalDetails = safeProperty(error, "technicalDetails") as string | null;
    return {
      code: stringProperty(error, "code"),
      summaryKey: stringProperty(error, "summaryKey") ?? "backendError.summary.generic",
      summaryParams: safeProperty(error, "summaryParams") as Record<string, string | number> | undefined,
      technicalDetails: technicalDetails
        ? redactBackendErrorDetails(technicalDetails)
        : null,
      recoverable: booleanProperty(error, "recoverable") ?? false,
      userActionRequired: booleanProperty(error, "userActionRequired") ?? false,
      actionKind: options.actionKindOverride !== undefined
        ? options.actionKindOverride
        : safeProperty(error, "actionKind") as BackendErrorActionKind,
    };
  }

  const objectError = typeof error === "object" && error !== null ? error : null;
  const code = objectError ? stringProperty(objectError, "code") : null;
  const message = objectError && isError(objectError)
    ? stringProperty(objectError, "message")
    : objectError
      ? stringProperty(objectError, "message")
      : typeof error === "string"
        ? error
        : null;
  const rawDetails = objectError ? safeProperty(objectError, "details") : undefined;
  const serializedDetails = rawDetails === undefined || rawDetails === null
    ? null
    : serializeTechnicalDetails(rawDetails);
  const serializedUnknown = objectError && !message && rawDetails === undefined
    ? serializeTechnicalDetails(objectError)
    : null;
  const serializedError = objectError && isError(objectError)
    ? serializeTechnicalDetails(objectError)
    : null;
  const combinedDetails = [
    code,
    message ? redactString(message) : null,
    serializedDetails,
    serializedUnknown,
    serializedError,
  ].filter((value): value is string => Boolean(value)).join("\n") || null;
  const technicalDetails = combinedDetails
    ? redactBackendErrorDetails(combinedDetails)
    : null;
  const recoverable = objectError
    ? booleanProperty(objectError, "recoverable") ?? options.defaultRecoverable ?? false
    : options.defaultRecoverable ?? false;
  const userActionRequired = objectError
    ? booleanProperty(objectError, "userActionRequired")
      ?? booleanProperty(objectError, "user_action_required")
      ?? options.defaultUserActionRequired
      ?? false
    : options.defaultUserActionRequired ?? false;
  const mapped = presentationForCode(code);
  const actionKind = options.actionKindOverride !== undefined
    ? options.actionKindOverride
    : mapped?.actionKind !== undefined
      ? mapped.actionKind
      : userActionRequired
        ? "open_settings"
        : options.defaultActionKind !== undefined
          ? options.defaultActionKind
          : defaultAction(recoverable, false, Boolean(technicalDetails));

  return {
    code,
    summaryKey: mapped?.summaryKey ?? options.defaultSummaryKey ?? "backendError.summary.generic",
    summaryParams: options.defaultSummaryParams,
    technicalDetails,
    recoverable,
    userActionRequired,
    actionKind,
  };
}

export function createActionableError(
  summaryKey: string,
  options: Omit<NormalizedBackendError, "code" | "summaryKey" | "technicalDetails"> & {
    technicalDetails?: string | null;
  },
): NormalizedBackendError {
  return {
    code: null,
    summaryKey,
    summaryParams: options.summaryParams,
    technicalDetails: options.technicalDetails
      ? redactBackendErrorDetails(options.technicalDetails)
      : null,
    recoverable: options.recoverable,
    userActionRequired: options.userActionRequired,
    actionKind: options.actionKind,
  };
}

export function translateBackendError(error: unknown, t: TFunction): string {
  const normalized = normalizeBackendError(error);
  return t(normalized.summaryKey, normalized.summaryParams);
}
