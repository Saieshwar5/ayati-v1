import { act, fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { Text } from 'react-native';
import { SessionProvider, useSession } from '../src/session/session-context';

const mockValues = new Map<string, string>();
jest.mock('expo-secure-store', () => ({
  getItemAsync: async (key: string) => mockValues.get(key) ?? null,
  setItemAsync: async (key: string, value: string) => void mockValues.set(key, value),
  deleteItemAsync: async (key: string) => void mockValues.delete(key),
}));
const mockSignIn = jest.fn();
jest.mock('../src/session/sign-in-flow', () => ({
  signInWithBrowser: (...args: unknown[]) => mockSignIn(...args),
  SignInCancelled: class extends Error {},
}));

function Probe() {
  const { state, signOut } = useSession();
  return (
    <>
      <Text testID="status">
        {state.status}
        {state.status === 'signed-in' ? `:${state.session.user.email}:${state.workspace?.name ?? 'none'}` : ''}
      </Text>
      <Text onPress={() => void signOut()}>sign out</Text>
    </>
  );
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

describe('the session on this phone', () => {
  beforeEach(() => {
    mockValues.clear();
    mockSignIn.mockReset();
  });

  it('is signed out without a stored token', async () => {
    const fetcher = jest.fn();
    await render(<SessionProvider fetcher={fetcher}><Probe /></SessionProvider>);
    await waitFor(() => expect(screen.getByTestId('status').props.children).toContain('signed-out'));
    expect(fetcher).not.toHaveBeenCalled();
  });

  it('restores the person and their last workspace, and forgets both on sign-out', async () => {
    mockValues.set('cuckoo.device-token', 'stored');
    mockValues.set('cuckoo.workspace.u1', 'w1');
    const fetcher = jest.fn(async (url: string) =>
      url.endsWith('/session')
        ? json({ user: { id: 'u1', email: 'asha@example.test' }, workspaces: [{ id: 'w1', name: 'Bakery', role: 'Admin' }] })
        : json({ ok: true }),
    );
    await render(<SessionProvider fetcher={fetcher as typeof fetch}><Probe /></SessionProvider>);
    await waitFor(() => expect(screen.getByTestId('status').props.children).toContain(':asha@example.test:Bakery'));
    await act(async () => fireEvent.press(screen.getByText('sign out')));
    await waitFor(() => expect(screen.getByTestId('status').props.children).toContain('signed-out'));
    expect(fetcher.mock.calls.some(([url]) => String(url).endsWith('/logout'))).toBe(true);
    expect(mockValues.has('cuckoo.device-token')).toBe(false);
    expect(mockValues.has('cuckoo.workspace.u1')).toBe(false);
  });

  it('signs out and forgets the token when the server no longer knows it', async () => {
    mockValues.set('cuckoo.device-token', 'revoked');
    const fetcher = jest.fn(async () => json({ error: 'Session ended or identity changed. Sign in again.' }, 401));
    await render(<SessionProvider fetcher={fetcher as unknown as typeof fetch}><Probe /></SessionProvider>);
    await waitFor(() => expect(screen.getByTestId('status').props.children).toContain('signed-out'));
    await waitFor(() => expect(mockValues.has('cuckoo.device-token')).toBe(false));
  });
});
