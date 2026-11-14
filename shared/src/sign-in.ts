/**
 * The phone app's half of sign-in (runtime/mobile-sign-in.md): the PKCE pair,
 * the address that starts sign-in and reading the address it returns to.
 * Randomness and SHA-256 come from the platform (expo-crypto on phones).
 */

const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_';

/** Base64url without padding (RFC 4648 §5), as PKCE's S256 method uses. */
export function base64url(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const [a, b = 0, c = 0] = [bytes[i], bytes[i + 1], bytes[i + 2]];
    const n = (a << 16) | (b << 8) | c;
    out += alphabet[(n >> 18) & 63] + alphabet[(n >> 12) & 63];
    if (i + 1 < bytes.length) out += alphabet[(n >> 6) & 63];
    if (i + 2 < bytes.length) out += alphabet[n & 63];
  }
  return out;
}

export type Pkce = { verifier: string; challenge: string };

/** A 43-character verifier from 32 random bytes, and its S256 challenge. */
export async function makePkce(
  randomBytes: (count: number) => Uint8Array,
  sha256: (text: string) => Promise<Uint8Array>,
): Promise<Pkce> {
  const verifier = base64url(randomBytes(32));
  return { verifier, challenge: base64url(await sha256(verifier)) };
}

export function startUrl(origin: string, challenge: string, options: { google?: boolean; email?: string } = {}): string {
  const params = new URLSearchParams({ challenge });
  if (options.google) params.set('method', 'google');
  if (options.email?.trim()) params.set('email', options.email.trim());
  return `${origin}/auth/mobile/start?${params}`;
}

/** Where sign-in hands the browser back: the universal link, or a development scheme. */
export function returnUrl(origin: string, scheme?: string): string {
  return scheme ? `${scheme}://auth/return` : `${origin}/auth/mobile/return`;
}

export type SignInResult = { code: string } | { error: string };

/** Read the code (or the failure) from the address sign-in returned to. */
export function readReturn(url: string): SignInResult {
  const query = url.includes('?') ? url.slice(url.indexOf('?') + 1).split('#')[0] : '';
  const params = new URLSearchParams(query);
  const code = params.get('code');
  if (code && /^[A-Za-z0-9_-]{20,100}$/.test(code)) return { code };
  return { error: params.get('error') ?? 'missing_code' };
}
