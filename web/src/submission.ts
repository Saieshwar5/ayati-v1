export interface Submission { request_id: string; content: string; attachments?: string[] }

// Keep one unacknowledged submission through retries/reloads. No automatic
// resend: the owner can retry the saved draft with the same receipt identity.
export class PendingSubmission {
  private pending?: Submission;
  private storage?: Storage;
  private readonly key: string;

  constructor(storage?: Storage, agent = 'local-agent') {
    this.key = agent === 'local-agent' ? 'ayati.pending-submission.v1' : `ayati.pending-submission.v1.${agent}`;
    this.storage = storage;
    try {
      const raw = storage?.getItem(this.key);
      if (raw) {
        const value = JSON.parse(raw) as Submission;
        if ((value.attachments === undefined || (Array.isArray(value.attachments) && value.attachments.length <= 8 && value.attachments.every(id => typeof id === 'string')))
          && typeof value.content === 'string' && value.content.length <= 16000
          && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value.request_id)) this.pending = value;
      }
    } catch { /* In-memory retry still works when browser storage is unavailable. */ }
  }

  draft(): string { return this.pending?.content ?? ''; }

  prepare(content: string, attachments: string[] = []): Submission {
    if (this.pending?.content !== content || JSON.stringify(this.pending?.attachments ?? []) !== JSON.stringify(attachments)) {
      this.pending = { content, request_id: crypto.randomUUID(), ...(attachments.length ? { attachments } : {}) };
      try { this.storage?.setItem(this.key, JSON.stringify(this.pending)); } catch { /* See above. */ }
    }
    return this.pending!;
  }

  acknowledge(requestId: string): void {
    if (this.pending?.request_id !== requestId) return;
    this.pending = undefined;
    try { this.storage?.removeItem(this.key); } catch { /* Storage is optional. */ }
  }
}
