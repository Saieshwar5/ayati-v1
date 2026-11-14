import type { Submission } from '@cuckoo/shared';
import { fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { Composer } from '../src/chats/composer';

let mockUuid = 0;
jest.mock('expo-crypto', () => ({ randomUUID: () => `id-${++mockUuid}` }));

describe('the composer', () => {
  beforeEach(() => {
    mockUuid = 0;
  });

  it('sends once on a double tap, to the conversation agent, and clears the draft', async () => {
    const sent: Submission[] = [];
    let finish: (value: { message?: undefined }) => void = () => {};
    const send = jest.fn((submission: Submission) => {
      sent.push(submission);
      return new Promise<{ message?: undefined }>(resolve => (finish = resolve));
    });
    await render(<Composer placeholder="Message Nova" agents={['nova']} send={send} sent={() => {}} />);
    await fireEvent.changeText(screen.getByLabelText('Message'), ' Hello Nova ');
    await fireEvent.press(screen.getByLabelText('Send message'));
    await fireEvent.press(screen.getByLabelText('Send message'));
    expect(send).toHaveBeenCalledTimes(1);
    expect(sent[0]).toEqual({ id: 'id-1', content: 'Hello Nova', agents: ['nova'], attachments: [] });
    finish({});
    await waitFor(() => expect(screen.getByLabelText('Message').props.value).toBe(''));
  });

  it('keeps the draft after a failure and retries with the same id', async () => {
    const send = jest
      .fn()
      .mockRejectedValueOnce(new Error('Workspace database unavailable. Retry shortly.'))
      .mockResolvedValueOnce({});
    await render(<Composer placeholder="Message" agents={[]} send={send} sent={() => {}} />);
    await fireEvent.changeText(screen.getByLabelText('Message'), 'Report please');
    await fireEvent.press(screen.getByLabelText('Send message'));
    expect(await screen.findByText(/Not sent · Workspace database unavailable/)).toBeTruthy();
    expect(screen.getByLabelText('Message').props.value).toBe('Report please');
    await fireEvent.press(screen.getByLabelText('Retry sending'));
    await waitFor(() => expect(send).toHaveBeenCalledTimes(2));
    expect(send.mock.calls[1][0].id).toBe(send.mock.calls[0][0].id);
    expect(send.mock.calls[0][0].agents).toBeUndefined();
  });
});
