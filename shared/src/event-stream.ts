/**
 * Server-Sent Events read from a streaming response body, for clients without
 * `EventSource` (React Native). Follows the WHATWG event-stream format: lines
 * end with LF, CR or CRLF; `event`, `data` and `id` fields; a blank line ends
 * an event; lines starting with `:` are comments.
 */
export type StreamEvent = { event: string; data: string; id?: string };

export class EventStreamParser {
  private buffer = '';
  private event = '';
  private data: string[] = [];
  private id: string | undefined;
  /** The last `id` seen, which a reconnect resumes after. */
  lastId: string | undefined;

  constructor(private readonly emit: (event: StreamEvent) => void) {}

  push(chunk: string): void {
    this.buffer += chunk;
    for (;;) {
      const match = /\r\n|\r|\n/.exec(this.buffer);
      if (!match) return;
      // A lone CR at the very end may be the first half of CRLF: wait for more.
      if (match[0] === '\r' && match.index === this.buffer.length - 1) return;
      const line = this.buffer.slice(0, match.index);
      this.buffer = this.buffer.slice(match.index + match[0].length);
      this.line(line);
    }
  }

  private line(line: string): void {
    if (line === '') {
      if (this.data.length) {
        this.emit({ event: this.event || 'message', data: this.data.join('\n'), id: this.id });
      }
      this.event = '';
      this.data = [];
      return;
    }
    if (line.startsWith(':')) return;
    const colon = line.indexOf(':');
    const field = colon < 0 ? line : line.slice(0, colon);
    let value = colon < 0 ? '' : line.slice(colon + 1);
    if (value.startsWith(' ')) value = value.slice(1);
    if (field === 'event') this.event = value;
    else if (field === 'data') this.data.push(value);
    else if (field === 'id' && !value.includes('\0')) {
      this.id = value;
      this.lastId = value;
    }
  }
}
