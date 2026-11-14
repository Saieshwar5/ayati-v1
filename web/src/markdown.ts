import { Marked } from 'marked';
import createDOMPurify from 'dompurify';
import { workspaceUrl } from './workspace-context';

const escape = (value: string) => value.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
const parser = new Marked({ gfm: true, breaks: true, renderer: {
  html: ({ text }) => escape(text),
  // Remote model-generated images must not make tracking requests. Uploaded media
  // uses the authenticated attachment renderer instead.
  image: ({ text }) => escape(text),
  // A fenced block gets a Copy button, handled by one listener per window (below).
  code: ({ text }) => `<pre class="message-code"><button type="button" class="message-code-copy" aria-label="Copy code">Copy</button><code>${escape(text)}</code></pre>`,
  link({ href, tokens }) {
    const text = this.parser.parseInline(tokens);
    if (/^mention:(user|agent):[a-zA-Z0-9_-]+$/.test(href)) return `<span class="message-mention">${text}</span>`;
    // A run Cuckoo posted from a schedule: opens it in Activity, same tab.
    if (/^run:[0-9a-f-]{36}$/.test(href)) return `<a href="${escape(workspaceUrl(`/?view=activity&tab=history&run=${href.slice(4)}`))}">${text}</a>`;
    // A Library citation Cuckoo adds: opens the item (at the cited revision) in this
    // workspace, same tab. Access is checked when it opens.
    const cited = href.match(/^\/w\/[0-9a-f-]{36}\/\?view=library&item=([0-9a-f-]{36})(?:&source_revision=([0-9]{1,9}))?$/);
    if (cited) return `<a href="${escape(workspaceUrl(`/?view=library&item=${cited[1]}${cited[2] ? `&source_revision=${cited[2]}` : ''}`))}">${text}</a>`;
    if (!/^(https?:|mailto:)/i.test(href)) return text;
    return `<a href="${escape(href)}" target="_blank" rel="noopener noreferrer">${text}</a>`;
  },
} });
let purifier: ReturnType<typeof createDOMPurify> | undefined;
let owner: Window | undefined;
/** Copies a code block's text. Delegated, so markup set as HTML needs no handlers of its own. */
function copyCode(event: MouseEvent) {
  const button = (event.target as Element | null)?.closest?.('button.message-code-copy');
  const code = button?.closest('pre')?.querySelector('code');
  if (!button || !code) return;
  const view = button.ownerDocument.defaultView;
  const say = (text: string) => { button.textContent = text; view?.setTimeout(() => { button.textContent = 'Copy'; }, 2000); };
  const clipboard = view?.navigator.clipboard;
  if (!clipboard) { say('Copy failed'); return; }
  void clipboard.writeText(code.textContent ?? '').then(() => say('Copied'), () => say('Copy failed'));
}
/** One HTML boundary for React and the remaining private transcript. Never bypass it. */
export function messageHtml(content: string): string {
  const current = document.defaultView!;
  if (owner !== current) { owner = current; purifier = createDOMPurify(current); current.document.addEventListener('click', copyCode); }
  return purifier!.sanitize(parser.parse(content, { async: false }), {
    ALLOWED_TAGS: ['p','br','strong','em','del','blockquote','h1','h2','h3','h4','h5','h6','ul','ol','li','pre','code','hr','table','thead','tbody','tr','th','td','a','span','button'],
    ALLOWED_ATTR: ['href','title','target','rel','class','start','type','aria-label'],
    ALLOW_DATA_ATTR: false,
  });
}
