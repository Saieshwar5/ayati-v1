import type { Attachment, Submission } from '@cuckoo/shared';
import { fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { Composer } from '../src/chats/composer';

let mockUuid = 0;
jest.mock('expo-crypto', () => ({ randomUUID: () => `id-${++mockUuid}` }));
const mockPickPhoto = jest.fn();
jest.mock('../src/chats/attachments', () => ({
  ...jest.requireActual('../src/chats/attachments'),
  pickPhoto: (...args: unknown[]) => mockPickPhoto(...args),
  pickFile: jest.fn(),
}));
jest.mock('expo-audio', () => {
  const recorder = { isRecording: false, uri: 'file:///cache/voice.m4a', prepareToRecordAsync: jest.fn(async () => {}),
    record: jest.fn(() => { recorder.isRecording = true; }), stop: jest.fn(async () => { recorder.isRecording = false; }) };
  return {
    AudioModule: { requestRecordingPermissionsAsync: async () => ({ granted: true }) },
    RecordingPresets: { HIGH_QUALITY: {} },
    setAudioModeAsync: async () => {},
    useAudioRecorder: () => recorder,
    useAudioRecorderState: () => ({ durationMillis: 3200, metering: -20 }),
  };
});

const file = (id: string, name: string): Attachment => ({ id, name, mime: 'image/jpeg', size: 10 });
const noStore = async (): Promise<Attachment> => {
  throw new Error('not used');
};

describe('the composer', () => {
  beforeEach(() => {
    mockUuid = 0;
    mockPickPhoto.mockReset();
  });

  it('sends once on a double tap, to the conversation agent, and clears the draft', async () => {
    const sent: Submission[] = [];
    let finish: (value: { message?: undefined }) => void = () => {};
    const send = jest.fn((submission: Submission) => {
      sent.push(submission);
      return new Promise<{ message?: undefined }>(resolve => (finish = resolve));
    });
    await render(<Composer placeholder="Message Nova" agents={['nova']} send={send} sent={() => {}} store={noStore} />);
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
    await render(<Composer placeholder="Message" agents={[]} send={send} sent={() => {}} store={noStore} />);
    await fireEvent.changeText(screen.getByLabelText('Message'), 'Report please');
    await fireEvent.press(screen.getByLabelText('Send message'));
    expect(await screen.findByText(/Not sent · Workspace database unavailable/)).toBeTruthy();
    expect(screen.getByLabelText('Message').props.value).toBe('Report please');
    await fireEvent.press(screen.getByLabelText('Retry sending'));
    await waitFor(() => expect(send).toHaveBeenCalledTimes(2));
    expect(send.mock.calls[1][0].id).toBe(send.mock.calls[0][0].id);
    expect(send.mock.calls[0][0].agents).toBeUndefined();
  });

  it('stores a chosen photo and shares it, even without text', async () => {
    mockPickPhoto.mockResolvedValue({ uri: 'file:///p.jpg', name: 'receipt.jpg', mime: 'image/jpeg', size: 2048 });
    const store = jest.fn(async () => file('f1', 'receipt.jpg'));
    const send = jest.fn(async (_submission: Submission) => ({}));
    await render(<Composer placeholder="Message" agents={[]} send={send} sent={() => {}} store={store} />);
    await fireEvent.press(screen.getByLabelText('Attach a photo or file'));
    await fireEvent.press(screen.getByLabelText('Attach from Photos'));
    expect(await screen.findByLabelText('Remove receipt.jpg')).toBeTruthy();
    expect(mockPickPhoto).toHaveBeenCalledWith('library');
    await fireEvent.press(screen.getByLabelText('Send message'));
    await waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(send.mock.calls[0][0]).toMatchObject({ content: 'Shared files', attachments: ['f1'] });
    await waitFor(() => expect(screen.queryByLabelText('Remove receipt.jpg')).toBeNull());
  });

  it('refuses a file over 10 MiB before uploading it', async () => {
    mockPickPhoto.mockResolvedValue({ uri: 'file:///big.jpg', name: 'big.jpg', mime: 'image/jpeg', size: 11 * 1024 * 1024 });
    const store = jest.fn();
    await render(<Composer placeholder="Message" agents={[]} send={jest.fn()} sent={() => {}} store={store} />);
    await fireEvent.press(screen.getByLabelText('Attach a photo or file'));
    await fireEvent.press(screen.getByLabelText('Attach from Camera'));
    expect(await screen.findByText(/Choose a file up to 10 MiB/)).toBeTruthy();
    expect(store).not.toHaveBeenCalled();
  });

  it('records a voice note, keeps it when its upload fails, and sends it on retry', async () => {
    const store = jest
      .fn()
      .mockRejectedValueOnce(new Error('File upload not confirmed; check stored records before retrying'))
      .mockResolvedValueOnce({ id: 'v1', name: 'voice-note.m4a', mime: 'audio/mp4', size: 900 });
    const send = jest.fn(async (_submission: Submission) => ({}));
    await render(<Composer placeholder="Message Nova" agents={['nova']} send={send} sent={() => {}} store={store} />);
    await fireEvent.press(screen.getByLabelText('Record a voice note'));
    expect(await screen.findByLabelText('Recording, 0:03 of 5:00')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Send the voice note'));
    expect(await screen.findByText(/Voice note not sent/)).toBeTruthy();
    expect(store.mock.calls[0][0]).toMatchObject({ uri: 'file:///cache/voice.m4a', mime: 'audio/mp4', name: 'voice-note.m4a' });
    expect(send).not.toHaveBeenCalled();
    await fireEvent.press(screen.getByLabelText('Retry sending'));
    await waitFor(() => expect(send).toHaveBeenCalledTimes(1));
    expect(store).toHaveBeenCalledTimes(2);
    expect(send.mock.calls[0][0]).toMatchObject({ content: 'Shared files', agents: ['nova'], attachments: ['v1'] });
  });
});
