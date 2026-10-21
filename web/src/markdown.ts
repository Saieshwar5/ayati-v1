import { Marked } from 'marked';
import createDOMPurify from 'dompurify';

const escape = (value: string) => value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
const parser = new Marked({ gfm: true, breaks: true, renderer: {
  html: ({ text }) => escape(text),
  // Remote model-generated images must not make tracking requests. Uploaded media
  // uses the authenticated attachment renderer instead.
  image: ({ text }) => escape(text),
  link({ href, tokens }) {
    const text = this.parser.parseInline(tokens);
    if (/^mention:(user|agent):[a-zA-Z0-9_-]+$/.test(href)) return `<span class="message-mention">${text}</span>`;
    if (!/^(https?:|mailto:)/i.test(href)) return text;
    return `<a href="${escape(href)}" target="_blank" rel="noopener noreferrer">${text}</a>`;
  },
} });
let purifier: ReturnType<typeof createDOMPurify> | undefined;
let owner: Window | undefined;
/** One HTML boundary for React and the remaining private transcript. Never bypass it. */
export function messageHtml(content: string): string {
  const current = document.defaultView!;
  if (owner !== current) { owner = current; purifier = createDOMPurify(current); }
  return purifier!.sanitize(parser.parse(content, { async: false }), {
    ALLOWED_TAGS: ['p','br','strong','em','del','blockquote','h1','h2','h3','h4','h5','h6','ul','ol','li','pre','code','hr','table','thead','tbody','tr','th','td','a','span'],
    ALLOWED_ATTR: ['href','title','target','rel','class','start'],
    ALLOW_DATA_ATTR: false,
  });
}
