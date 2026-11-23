export type ResponseReference = {
  source_scope?: string | null;
  schema: 1; id: string; version: string; sequence: number;
  state: 'complete' | 'draft' | 'interrupted'; characters: number; bytes: number;
  long: boolean; available: boolean;
  /** Set when the reply holds a page; the reply text then excludes it. */
  page?: import('../pages/page-format').PageSummary;
};
export type SavedResponse = ResponseReference & { body: string; format: 'markdown' };
export type SavedFile = { id: string; name: string; size: number; mime: string; checksum: string };
export type ResultSource = {
  read: <T>(path: string, signal: AbortSignal) => Promise<T>;
  href: (path: string) => string;
  response: (id: string) => string;
  file: (id: string) => string;
  subscribe?: (check: (scope?: string | null) => void) => () => void;
};
export type Selection = { kind: 'response'; id: string } | { kind: 'page'; id: string } | { kind: 'file' | 'image'; id: string; name: string };
/** The pictures the image viewer shows itself; other image types open as files. */
export const isViewerImage = (mime: string) => mime === 'image/png' || mime === 'image/jpeg';
/** A picture of the conversation the image viewer was opened from: who shared it and when (seconds). */
export type ViewerImage = { id: string; name: string; by?: string; at?: number; face?: import('react').ReactNode };
export const query = (path: string, values: Record<string, string | boolean>) => `${path}${path.includes('?') ? '&' : '?'}${new URLSearchParams(Object.entries(values).map(([key, value]) => [key, String(value)]))}`;
export function responsePreview(text: string, reference?: ResponseReference) {
  const characters = [...text];
  const long = Boolean(reference?.long) || characters.length > 900 || text.split('\n', 10).length > 9;
  return { text: long ? characters.slice(0, 320).join('') : text, long };
}
export const denied = (error: unknown) => Boolean(error && typeof error === 'object' && 'status' in error && [400, 401, 403, 404].includes(Number(error.status)));
