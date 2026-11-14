import type { Conversation, Directory } from '@cuckoo/shared';
import { fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { AgentProfileView } from '../src/agents/agent-profile-view';

const mockCreateChat = jest.fn();
jest.mock('../src/faces/face', () => ({ Face: () => null }));
jest.mock('expo-crypto', () => ({ randomUUID: () => 'new-chat-id' }));
jest.mock('../src/session/session-context', () => ({
  useWorkspace: () => ({ user: { id: 'me' }, workspace: { id: 'w' }, api: { createChat: mockCreateChat } }),
}));

const directory: Directory = {
  role: 'Member',
  users: [{ id: 'me', name: 'Me' }, { id: 'boss', name: 'Asha Rao' }],
  agents: [{ id: 'nova', name: 'Nova', title: 'Bookkeeper', about: 'Keeps the books tidy.', manager: 'boss' }],
};
const channel: Conversation = { id: 'c1', name: 'finance', kind: 'channel', description: '', users: ['me'], agents: ['nova'] };

describe('an agent profile', () => {
  beforeEach(() => mockCreateChat.mockReset());

  it('shows who manages it, what it does and its channels', async () => {
    await render(<AgentProfileView agentId="nova" directory={directory} chats={[channel]} opened={() => {}} />);
    expect(screen.getByText('Bookkeeper')).toBeTruthy();
    expect(screen.getByText('Managed by Asha Rao')).toBeTruthy();
    expect(screen.getByText('Keeps the books tidy.')).toBeTruthy();
    expect(screen.getByText('# finance')).toBeTruthy();
  });

  it('opens the existing direct conversation without creating another', async () => {
    const opened = jest.fn();
    const direct: Conversation = { id: 'd1', name: 'Nova', kind: 'direct', description: '', users: ['me'], agents: ['nova'] };
    await render(<AgentProfileView agentId="nova" directory={directory} chats={[channel, direct]} opened={opened} />);
    await fireEvent.press(screen.getByLabelText('Message Nova'));
    await waitFor(() => expect(opened).toHaveBeenCalledWith('d1'));
    expect(mockCreateChat).not.toHaveBeenCalled();
  });

  it('creates the conversation once, even on a double tap', async () => {
    let finish: (value: { id: string }) => void = () => {};
    mockCreateChat.mockImplementation(() => new Promise(resolve => (finish = resolve)));
    const opened = jest.fn();
    await render(<AgentProfileView agentId="nova" directory={directory} chats={[channel]} opened={opened} />);
    await fireEvent.press(screen.getByLabelText('Message Nova'));
    await fireEvent.press(screen.getByLabelText('Message Nova'));
    expect(mockCreateChat).toHaveBeenCalledTimes(1);
    expect(mockCreateChat.mock.calls[0][0]).toEqual({ id: 'new-chat-id', kind: 'direct', name: 'Nova', users: [], agents: ['nova'] });
    finish({ id: 'new-chat-id' });
    await waitFor(() => expect(opened).toHaveBeenCalledWith('new-chat-id'));
  });

  it('says so when the agent is gone', async () => {
    await render(<AgentProfileView agentId="gone" directory={directory} chats={[]} opened={() => {}} />);
    expect(screen.getByText('This agent is no longer available to you.')).toBeTruthy();
  });
});
