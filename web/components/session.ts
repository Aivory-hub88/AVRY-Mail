// Session expiry. Sessions are 7-day JWTs. Without this, an expired token made
// every API call answer 401 while the app kept polling and showed an empty
// inbox, which looks exactly like lost mail. Now the first 401 (or a token
// already past its `exp` at load) ends the session and goes to /login.

const STORAGE_KEYS = ["aivory_mail_token", "aivory_mail_email"];

/** `exp` (unix seconds) of a JWT, or null when it can't be read. */
export function tokenExpiry(token: string | null | undefined): number | null {
  if (!token) return null;
  const part = token.split(".")[1];
  if (!part) return null;
  try {
    const b64 = part.replace(/-/g, "+").replace(/_/g, "/");
    const json = typeof atob === "function" ? atob(b64) : Buffer.from(b64, "base64").toString("binary");
    const exp = JSON.parse(json).exp;
    return typeof exp === "number" ? exp : null;
  } catch {
    return null;
  }
}

/**
 * True only when the token says it has expired. An unreadable token is not
 * "expired" here: the server decides, and its 401 ends the session.
 */
export function tokenExpired(token: string | null | undefined, nowMs: number = Date.now()): boolean {
  const exp = tokenExpiry(token);
  return exp !== null && exp * 1000 <= nowMs;
}

let ending = false;

/** Forget the session and send the whole window (not just an iframe) to login. */
export function endExpiredSession(): void {
  if (typeof window === "undefined" || ending) return;
  ending = true;
  for (const key of STORAGE_KEYS) {
    try { localStorage.removeItem(key); } catch {}
    try { sessionStorage.removeItem(key); } catch {}
  }
  if (window.location.pathname === "/login") return;
  const target = "/login?expired=1";
  try {
    (window.top || window).location.assign(target);
  } catch {
    window.location.assign(target);
  }
}

/** Use as `fetch(...).then(watchUnauthorized)`: a 401 means the session is over. */
export function watchUnauthorized(res: Response): Response {
  if (res.status === 401) endExpiredSession();
  return res;
}
