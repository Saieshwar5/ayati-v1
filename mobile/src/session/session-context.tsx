/**
 * Who is signed in on this phone and which workspace they are in. It changes
 * rarely (sign-in, sign-out, workspace switch), so one context is cheap.
 * Screens below are keyed by user and workspace, so switching discards their
 * data, drafts and live connection.
 */
import { createClient, type Client, type Membership, type Session } from '@cuckoo/shared';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { origin } from '../config';
import { forgetToken, forgetWorkspace, readToken, readWorkspace, saveToken, saveWorkspace } from './secure-values';
import { signInWithBrowser } from './sign-in-flow';

export type SessionState =
  | { status: 'loading' }
  | { status: 'signed-out'; error?: string }
  | { status: 'signed-in'; session: Session & { user: NonNullable<Session['user']> }; workspace?: Membership };

type SessionValue = {
  state: SessionState;
  client: Client;
  signIn: (options?: { google?: boolean }) => Promise<void>;
  signOut: () => Promise<void>;
  chooseWorkspace: (id: string | undefined) => Promise<void>;
  refresh: () => Promise<void>;
  /** The device-session token, for requests the shared client does not make (the live stream). */
  token: () => string | undefined;
};

const SessionContext = createContext<SessionValue | undefined>(undefined);

export function SessionProvider({ children, fetcher = fetch }: { children: ReactNode; fetcher?: typeof fetch }) {
  const [state, setState] = useState<SessionState>({ status: 'loading' });
  const token = useRef<string | undefined>(undefined);
  // Each sign-in or sign-out starts a generation; older answers are ignored.
  const generation = useRef(0);

  // Stable, so the live stream does not restart when the session state changes.
  const currentToken = useCallback(() => token.current, []);

  const signedOut = useCallback((error?: string) => {
    generation.current++;
    token.current = undefined;
    void forgetToken().catch(() => {});
    setState({ status: 'signed-out', error });
  }, []);

  const client = useMemo(
    () =>
      createClient({
        origin,
        fetch: (input, init) => fetcher(input, init),
        token: currentToken,
        onUnauthorized: () => {
          if (token.current) signedOut('You were signed out. Sign in again.');
        },
      }),
    [fetcher, signedOut],
  );

  const load = useCallback(async () => {
    const request = generation.current;
    const session = await client.session();
    if (generation.current !== request) return;
    if (!session.user) return signedOut();
    const user = session.user;
    const saved = await readWorkspace(user.id).catch(() => null);
    if (generation.current !== request) return;
    const workspace = session.workspaces?.find(w => w.id === saved);
    setState({ status: 'signed-in', session: { ...session, user }, workspace });
  }, [client, signedOut]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const stored = await readToken().catch(() => null);
      if (cancelled) return;
      if (!stored) return setState({ status: 'signed-out' });
      token.current = stored;
      try {
        await load();
      } catch {
        // Offline at launch: keep the token and offer to try again from sign-in.
        if (!cancelled) setState({ status: 'signed-out', error: 'Cuckoo could not be reached. Check your connection.' });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [load]);

  const value = useMemo<SessionValue>(
    () => ({
      state,
      client,
      refresh: load,
      token: () => token.current,
      signIn: async options => {
        const issued = await signInWithBrowser(client, options);
        generation.current++;
        token.current = issued.token;
        await saveToken(issued.token);
        await load();
      },
      signOut: async () => {
        // The server ends the device session and its WorkOS session; the phone
        // forgets everything either way.
        await client.logout().catch(() => {});
        if (state.status === 'signed-in') await forgetWorkspace(state.session.user.id).catch(() => {});
        signedOut();
      },
      chooseWorkspace: async id => {
        if (state.status !== 'signed-in') return;
        const workspace = state.session.workspaces?.find(w => w.id === id);
        if (workspace) await saveWorkspace(state.session.user.id, workspace.id);
        else await forgetWorkspace(state.session.user.id);
        setState({ ...state, workspace });
      },
    }),
    [state, client, load, signedOut, currentToken],
  );

  return <SessionContext.Provider value={value}>{children}</SessionContext.Provider>;
}

export function useSession(): SessionValue {
  const value = useContext(SessionContext);
  if (!value) throw new Error('useSession must be used inside SessionProvider');
  return value;
}

/** The signed-in person and their current workspace; only for screens behind the workspace guard. */
export function useWorkspace() {
  const { state, client } = useSession();
  if (state.status !== 'signed-in' || !state.workspace) throw new Error('No workspace is open');
  const workspace = state.workspace;
  const api = useMemo(() => client.workspace(workspace.id), [client, workspace.id]);
  return { user: state.session.user, workspace, api, client };
}
