export type ResponseReference = {
  schema: 1; id: string; version: string; sequence: number;
  state: 'complete' | 'draft' | 'interrupted'; characters: number; bytes: number;
  long: boolean; available: boolean;
};
export type SavedResponse = ResponseReference & { body: string; format: 'markdown' };
export type SavedFile = { id: string; name: string; size: number; mime: string; checksum: string };
export type ResultSource = {
  read: <T>(path: string, signal: AbortSignal) => Promise<T>;
  href: (path: string) => string;
  response: (id: string) => string;
  file: (id: string) => string;
  subscribe?: (check: () => void) => () => void;
};
export type Selection = { kind: 'response'; id: string } | { kind: 'file'; id: string; name: string };
export const query = (path: string, values: Record<string, string | boolean>) => `${path}${path.includes('?') ? '&' : '?'}${new URLSearchParams(Object.entries(values).map(([key, value]) => [key, String(value)]))}`;
export function responsePreview(text: string, reference?: ResponseReference) {
  const characters = [...text];
  const long = Boolean(reference?.long) || characters.length > 900 || text.split('\n', 10).length > 9;
  return { text: long ? characters.slice(0, 320).join('') : text, long };
}
export const denied = (error: unknown) => Boolean(error && typeof error === 'object' && 'status' in error && [400, 401, 403, 404].includes(Number(error.status)));
