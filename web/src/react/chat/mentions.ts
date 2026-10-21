export type Mention = { id: string; name: string; kind: 'user' | 'agent' };
export type MentionRange = Mention & { start: number; end: number };
const pattern = /\[@([^\]\n]+)\]\(mention:(user|agent):([a-zA-Z0-9_-]+)\)/g;
export function decodeMentions(source: string): { text: string; mentions: MentionRange[] } {
  let text = '', end = 0;
  const mentions: MentionRange[] = [];
  for (const match of source.matchAll(pattern)) {
    text += source.slice(end, match.index);
    const start = text.length; text += `@${match[1]}`;
    mentions.push({ name: match[1], kind: match[2] as Mention['kind'], id: match[3], start, end: text.length });
    end = match.index! + match[0].length;
  }
  return { text: text + source.slice(end), mentions };
}
export function encodeMentions(text: string, mentions: MentionRange[]): string {
  let result = '', at = 0;
  for (const mention of [...mentions].sort((a,b) => a.start-b.start)) {
    result += text.slice(at, mention.start) + `[@${mention.name}](mention:${mention.kind}:${mention.id})`; at = mention.end;
  }
  return result + text.slice(at);
}
/** Edits inside a mention turn it into ordinary text; surrounding edits move it. */
export function editMentions(source: string, next: string): string {
  const { text, mentions } = decodeMentions(source);
  let start = 0, tail = 0;
  while (start < Math.min(text.length, next.length) && text[start] === next[start]) start++;
  while (tail < Math.min(text.length, next.length)-start && text[text.length-tail-1] === next[next.length-tail-1]) tail++;
  const end = text.length-tail, shift = next.length-text.length;
  const kept = mentions.flatMap(m => m.end <= start ? [m] : m.start >= end ? [{ ...m, start: m.start+shift, end: m.end+shift }] : []);
  return encodeMentions(next, kept);
}
export function requestAgent(source: string, direct = ''): string | undefined {
  const agents = [...new Set(decodeMentions(source).mentions.filter(m => m.kind === 'agent').map(m => m.id))];
  if (agents.length > 1 || (direct && agents.some(id => id !== direct))) throw new Error('Ask one agent per message. Send a separate request for another agent.');
  return direct || agents[0];
}
