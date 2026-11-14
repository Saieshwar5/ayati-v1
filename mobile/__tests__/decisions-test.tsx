import type { Approval, Question } from '@cuckoo/shared';
import { fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { ApprovalReview } from '../src/decisions/approval-review';
import { QuestionReview } from '../src/decisions/question-review';

const mockApi = { approval: jest.fn(), decide: jest.fn(), question: jest.fn(), answer: jest.fn() };
let mockUuid = 0;
jest.mock('expo-crypto', () => ({ randomUUID: () => `request-${++mockUuid}` }));
jest.mock('expo-router', () => ({ useRouter: () => ({ back: jest.fn() }) }));
jest.mock('../src/live/live-context', () => ({ useChanges: () => {} }));
jest.mock('../src/session/session-context', () => ({
  useWorkspace: () => ({ user: { id: 'me' }, workspace: { id: 'w' }, api: mockApi }),
}));

const approval = (overrides: Partial<Approval> = {}): Approval => ({
  id: 'a1', title: 'Send invoice INV-7', operation: 'send_invoice', effect: 'write', state: 'pending', chat: 'c', thread: null,
  agent: 'nova', agent_name: 'Nova', account_name: 'Billing', can_decide: true,
  arguments: { invoice_id: 'INV-7', amount: '€120.00', recipient: 'ana@example.test' }, ...overrides,
});

describe('approving an exact action', () => {
  beforeEach(() => {
    mockUuid = 0;
    Object.values(mockApi).forEach(fn => fn.mockReset());
  });

  it('shows the exact fields and sends one decision with a request id kept for retries', async () => {
    mockApi.approval.mockResolvedValue(approval());
    mockApi.decide.mockRejectedValueOnce(new Error('Authentication service unavailable. Try again shortly.')).mockResolvedValueOnce({});
    await render(<ApprovalReview id="a1" />);
    expect(await screen.findByText('Send invoice INV-7')).toBeTruthy();
    expect(screen.getByText('invoice id')).toBeTruthy();
    expect(screen.getByText('ana@example.test')).toBeTruthy();
    expect(screen.getByText('Approving allows this one action exactly as shown. The first decision counts.')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Approve'));
    expect(await screen.findByText('Authentication service unavailable. Try again shortly.')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Approve'));
    await waitFor(() => expect(mockApi.decide).toHaveBeenCalledTimes(2));
    expect(mockApi.decide.mock.calls[0]).toEqual(['a1', 'approve', 'request-1']);
    expect(mockApi.decide.mock.calls[1]).toEqual(['a1', 'approve', 'request-1']);
  });

  it('offers no decision to someone who may not decide, and says whom it waits for', async () => {
    mockApi.approval.mockResolvedValue(approval({ can_decide: false, deciders: [{ id: 'r', name: 'Ravi' }, { id: 'm', name: 'Meera' }] }));
    await render(<ApprovalReview id="a1" />);
    expect(await screen.findByText('Waiting for Ravi or Meera')).toBeTruthy();
    expect(screen.queryByLabelText('Approve')).toBeNull();
    expect(screen.queryByLabelText('Decline')).toBeNull();
  });

  it('shows the outcome once someone has decided', async () => {
    mockApi.approval.mockResolvedValue(approval({ state: 'rejected', decision: 'reject', decided_by_name: 'Ravi', can_decide: false }));
    await render(<ApprovalReview id="a1" />);
    expect(await screen.findByText('Rejected by Ravi')).toBeTruthy();
    expect(screen.queryByLabelText('Approve')).toBeNull();
  });
});

const question = (overrides: Partial<Question> = {}): Question => ({
  id: 'q1', key: 'k', revision: 3, state: 'pending', values: null, respondent: 'me', respondent_name: 'Me', can_respond: true,
  reply: 'r', chat: 'c',
  specification: { version: 1, kind: 'single_choice', title: 'Which day should I book?', description: 'The studio is free twice.',
    fallback: '', allow_other: false, options: [{ id: 'mon', label: 'Monday' }, { id: 'tue', label: 'Tuesday' }], fields: [] },
  ...overrides,
});

describe('answering an agent question', () => {
  beforeEach(() => {
    mockUuid = 0;
    Object.values(mockApi).forEach(fn => fn.mockReset());
  });

  it('asks for a choice before sending, then sends it once with the question revision', async () => {
    mockApi.question.mockResolvedValue(question());
    mockApi.answer.mockResolvedValue({ state: 'answered', values: { choices: ['tue'] } });
    await render(<QuestionReview id="q1" />);
    expect(await screen.findByText('Which day should I book?')).toBeTruthy();
    expect(screen.getByText('Answering is not an approval.')).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Send answer'));
    expect(await screen.findByText('Choose an answer.')).toBeTruthy();
    expect(mockApi.answer).not.toHaveBeenCalled();
    await fireEvent.press(screen.getByLabelText('Tuesday'));
    await fireEvent.press(screen.getByLabelText('Send answer'));
    await waitFor(() => expect(mockApi.answer).toHaveBeenCalledTimes(1));
    expect(mockApi.answer.mock.calls[0]).toEqual(['q1', { id: 'request-1', revision: 3, decision: 'answer', values: { choices: ['tue'] } }]);
    expect(await screen.findByText('Tuesday')).toBeTruthy();
    expect(screen.queryByLabelText('Send answer')).toBeNull();
  });

  it('shows the agent fallback for a kind this app cannot answer', async () => {
    mockApi.question.mockResolvedValue(question({
      specification: { ...question().specification, kind: 'signature', fallback: 'Sign on the web, please.' },
    }));
    await render(<QuestionReview id="q1" />);
    expect(await screen.findByText('Sign on the web, please.')).toBeTruthy();
    expect(screen.queryByLabelText('Send answer')).toBeNull();
  });
});
