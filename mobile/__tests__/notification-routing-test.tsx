import { render, waitFor } from '@testing-library/react-native';
import { NotificationRouting, pathFor } from '../src/notifications/notification-routing';

const mockPush = jest.fn();
const mockChoose = jest.fn();
let mockResponse: unknown = null;
let mockState: unknown;
jest.mock('expo-router', () => ({ useRouter: () => ({ push: mockPush }) }));
jest.mock('expo-notifications', () => ({
  DEFAULT_ACTION_IDENTIFIER: 'expo.modules.notifications.actions.DEFAULT',
  setNotificationHandler: jest.fn(),
  useLastNotificationResponse: () => mockResponse,
  clearLastNotificationResponseAsync: async () => {},
}));
jest.mock('../src/session/session-context', () => ({
  useSession: () => ({ state: mockState, chooseWorkspace: mockChoose }),
}));

const tap = (data: unknown) => ({
  actionIdentifier: 'expo.modules.notifications.actions.DEFAULT',
  notification: { request: { content: { data } } },
});
const signedIn = (current: string) => ({
  status: 'signed-in',
  session: { user: { id: 'me' }, workspaces: [{ id: 'bakery' }, { id: 'cafe' }] },
  workspace: { id: current },
});

describe('opening a notification', () => {
  beforeEach(() => {
    mockPush.mockReset();
    mockChoose.mockReset();
  });

  it('opens the exact conversation, approval or question', async () => {
    expect(pathFor({ workspace: 'w', kind: 'approval', approval: 'a1' })).toEqual({ pathname: '/approval/[id]', params: { id: 'a1' } });
    mockState = signedIn('bakery');
    mockResponse = tap({ workspace: 'bakery', kind: 'message', chat: 'c1' });
    await render(<NotificationRouting />);
    await waitFor(() => expect(mockPush).toHaveBeenCalledWith({ pathname: '/chat/[id]', params: { id: 'c1' } }));
    expect(mockChoose).not.toHaveBeenCalled();
  });

  it('switches to the right workspace first', async () => {
    mockState = signedIn('bakery');
    mockResponse = tap({ workspace: 'cafe', kind: 'question', question: 'q1' });
    await render(<NotificationRouting />);
    await waitFor(() => expect(mockChoose).toHaveBeenCalledWith('cafe'));
    expect(mockPush).not.toHaveBeenCalled();
  });

  it('opens nothing for a workspace the person left, or for malformed data', async () => {
    mockState = signedIn('bakery');
    mockResponse = tap({ workspace: 'gone', kind: 'message', chat: 'c1' });
    const first = await render(<NotificationRouting />);
    first.unmount();
    mockResponse = tap({ workspace: 'bakery', kind: 'message', chat: '../../settings' });
    await render(<NotificationRouting />);
    await new Promise(resolve => setTimeout(resolve, 20));
    expect(mockPush).not.toHaveBeenCalled();
    expect(mockChoose).not.toHaveBeenCalled();
  });
});
