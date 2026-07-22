const SENSITIVE_KEY_PARTS = [
  "accesstoken", "authorization", "cookie", "credential", "csrf", "expires", "hdnts",
  "mstoken", "passportcsrftoken", "secret", "session", "sessdata", "sidguard", "sign",
  "signature", "token", "ttwid", "verifyfp", "websession", "xbogus", "xsectoken",
];

function normalizedKey(value) {
  return String(value).replace(/[^a-z0-9]/gi, "").toLowerCase();
}

export function isSensitiveKey(value) {
  const key = normalizedKey(value);
  return SENSITIVE_KEY_PARTS.some((part) => key === part || key.endsWith(part));
}

export function redactJsonValue(value) {
  if (Array.isArray(value)) return value.map(redactJsonValue);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, child]) => [
      key,
      isSensitiveKey(key) ? "REDACTED" : redactJsonValue(child),
    ]));
  }
  return typeof value === "string" ? redactSensitiveText(value) : value;
}

export function redactSensitiveText(value) {
  return String(value)
    .replace(/(<meta\b(?=[^>]*\b(?:name|property)=["'][^"']*(?:token|csrf|auth|session|cookie|secret|signature)[^"']*["'])[^>]*\bcontent=["'])[^"']*/gi, "$1REDACTED")
    .replace(/([?&](?:access_token|authorization|cookie|credential|expires|expire|msToken|mstoken|secret|sessionid|sid_guard|web_session|sessdata|auth_key|hdnts|sign|signature|token|xsec_token|xsec-token|x-bogus|a_bogus|verifyFp|verify_fp|ttwid|odin_tt|passport_csrf_token)=)[^&\s"'<>]*/gi, "$1REDACTED")
    .replace(/(["'][a-z0-9_-]*(?:token|secret|session|cookie|authorization|credential|signature|csrf)[a-z0-9_-]*["']\s*:\s*["'])[^"']*/gi, "$1REDACTED")
    .replace(/((?:^|[\s{;,])(?:data-)?[a-z0-9_-]*(?:token|secret|session|cookie|authorization|credential|signature|csrf)[a-z0-9_-]*\s*[:=]\s*["']?)[^"'&\s<>;,}]+/gi, "$1REDACTED");
}

export function isLoginChallengeState(url, bodyText) {
  const combined = `${url || ""}\n${bodyText || ""}`;
  return /captcha|challenge|passport|\/login(?:[/?#]|$)|login required|sign in|扫码登录|登录后|请登录|环境异常|访问过于频繁|请完成验证|去验证/i.test(combined);
}
