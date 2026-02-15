const TOKEN_KEY = "sso_access_token";
const REFRESH_MARGIN_SEC = 90; // redirect 1.5 min before expiry

// Global SSO proxy URL (set once from /api/v1/auth/config)
let _ssoProxyUrl: string | null = null;

export function setSsoProxyUrl(url: string | null): void {
  _ssoProxyUrl = url;
}

export function getSsoProxyUrl(): string | null {
  return _ssoProxyUrl;
}

/** Parse JWT payload without signature verification (client-side only). */
function parseJwt(token: string): Record<string, unknown> | null {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return null;
    const b64 = parts[1].replace(/-/g, "+").replace(/_/g, "/");
    const payload = atob(b64);
    return JSON.parse(payload);
  } catch {
    return null;
  }
}

/** Get stored token if present and not expired, null otherwise. */
export function getToken(): string | null {
  const token = localStorage.getItem(TOKEN_KEY);
  if (!token) return null;
  const parsed = parseJwt(token);
  if (!parsed || typeof parsed.exp !== "number") return null;
  if (parsed.exp <= Math.floor(Date.now() / 1000)) {
    localStorage.removeItem(TOKEN_KEY);
    return null;
  }
  return token;
}

/** Check URL for ?token= param, store it in localStorage, clean URL. */
export function captureTokenFromUrl(): boolean {
  const params = new URLSearchParams(window.location.search);
  const token = params.get("token");
  if (!token) return false;
  localStorage.setItem(TOKEN_KEY, token);
  params.delete("token");
  const qs = params.toString();
  const newUrl = qs
    ? `${window.location.pathname}?${qs}`
    : window.location.pathname;
  window.history.replaceState({}, "", newUrl);
  return true;
}

/** Redirect browser to SSO proxy for token acquisition. */
export function redirectToSso(proxyUrl: string): void {
  const url = new URL(proxyUrl);
  url.searchParams.set("redirect_to", window.location.href);
  window.location.href = url.toString();
}

/** Start periodic token expiry check. Returns cleanup function. */
export function startTokenRefresh(proxyUrl: string): () => void {
  const id = setInterval(() => {
    const token = localStorage.getItem(TOKEN_KEY);
    if (!token) return;
    const parsed = parseJwt(token);
    if (!parsed || typeof parsed.exp !== "number") return;
    const remaining = parsed.exp - Math.floor(Date.now() / 1000);
    if (remaining < REFRESH_MARGIN_SEC) {
      redirectToSso(proxyUrl);
    }
  }, 60_000);
  return () => clearInterval(id);
}

/** Remove stored token. */
export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

/** Extract username from JWT token (preferred_username or sub). */
export function getTokenUsername(): string | null {
  const token = getToken();
  if (!token) return null;
  const parsed = parseJwt(token);
  if (!parsed) return null;
  if (typeof parsed.preferred_username === "string")
    return parsed.preferred_username;
  if (typeof parsed.sub === "string") return parsed.sub;
  return null;
}
