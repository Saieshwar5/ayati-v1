import type { Message } from '@cuckoo/shared';
import { canStop } from '@cuckoo/shared';
import { fireEvent, render, screen } from '@testing-library/react-native';
import { MessageBubble } from '../src/chats/message-bubble';

jest.mock('../src/faces/face', () => ({ Face: () => null }));
jest.mock('../src/chats/attachment-view', () => ({ AttachmentList: () => null }));

const reply = (overrides: Partial<Message> = {}): Message => ({
  id: 'r1', author: 'nova', name: 'Nova', kind: 'agent', content: '', state: 'queued', at: 1, can_cancel: true,
  progress: { state: 'responding', text: 'Step 2 of 4 · Reading invoices' }, ...overrides,
});

describe('stopping an agent', () => {
  it('is offered only for a reply still in progress that the viewer may stop', () => {
    expect(canStop(reply())).toBe(true);
    expect(canStop(reply({ can_cancel: false }))).toBe(false);
    expect(canStop(reply({ state: 'complete' }))).toBe(false);
  });

  it('shows the step in progress and a Stop button that names the agent', async () => {
    const onStop = jest.fn();
    const message = reply();
    await render(<MessageBubble message={message} mine={false} agent face={{ name: 'Nova' }} workspace="w" onStop={onStop} />);
    expect(screen.getByText('Step 2 of 4 · Reading invoices')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Stop Nova'));
    expect(onStop).toHaveBeenCalledWith(message);
  });

  it('disables Stop while the request is being stopped', async () => {
    await render(<MessageBubble message={reply()} mine={false} agent face={{ name: 'Nova' }} workspace="w" onStop={() => {}} stopping />);
    expect(screen.getByText('Stopping…')).toBeTruthy();
    expect(screen.getByLabelText('Stop Nova').props.accessibilityState).toMatchObject({ disabled: true });
  });
});
