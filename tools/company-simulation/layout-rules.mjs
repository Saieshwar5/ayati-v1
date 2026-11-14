// Layout rules evaluated inside the page with page.evaluate(auditLayout, options).
// The function must stay self-contained: Playwright serializes it without imports or closures.
// Rules and their deliberate simplifications are documented in runtime/layout-and-speed-checks.md.
export function auditLayout({ minTarget, tolerance, scope }) {
  // An open modal (such as the thread drawer) is audited on its own; the page behind is inert.
  const root = (scope && document.querySelector(scope)) || document.body;
  const width = window.innerWidth, findings = [], styles = new Map();
  const style = el => { if (!styles.has(el)) styles.set(el, getComputedStyle(el)); return styles.get(el); };
  const clean = text => (text ?? '').replace(/\s+/g, ' ').trim().slice(0, 70);
  const nameOf = el => clean(el.getAttribute('aria-label') || el.labels?.[0]?.textContent || el.getAttribute('title')
    || el.getAttribute('placeholder') || el.textContent || el.getAttribute('name') || el.getAttribute('type'));
  const selectorOf = el => el.tagName.toLowerCase() + (el.getAttribute('role') ? `[role=${el.getAttribute('role')}]` : '')
    + [...el.classList].slice(0, 2).map(name => `.${name}`).join('');
  // The nearest labelled region (navigation, dialog, list…) tells the owner where to look.
  const areaOf = el => {
    const region = el.parentElement?.closest('nav[aria-label],aside[aria-label],main[aria-label],section[aria-label],dialog,[role=dialog],[role=toolbar],header,footer,form');
    if (!region) return '';
    const label = region.getAttribute('aria-label') || region.querySelector('h1,h2,h3')?.textContent;
    return clean(`${region.tagName.toLowerCase()}${label ? ` "${clean(label).slice(0, 40)}"` : ''}`);
  };
  const box = rect => ({ x: Math.round(rect.left), y: Math.round(rect.top), width: Math.round(rect.width), height: Math.round(rect.height) });
  const mark = el => { el.dataset.layoutAudit = String(findings.length); };
  const report = (rule, el, rect, detail) => {
    mark(el);
    findings.push({ rule, selector: selectorOf(el), name: nameOf(el), area: areaOf(el), box: box(rect), detail });
  };
  // Ancestors with overflow hidden/clip cut an element's visible box; scroll containers do not,
  // because their content stays reachable. Fixed elements escape their ancestors' clipping.
  const clipping = el => {
    let right = Infinity, left = -Infinity, top = -Infinity, bottom = Infinity, scrolls = false;
    for (let node = el; node && node !== document.documentElement; node = node.parentElement) {
      const s = style(node);
      if (node !== el) {
        const rect = node.getBoundingClientRect();
        if (['hidden', 'clip'].includes(s.overflowX)) { left = Math.max(left, rect.left); right = Math.min(right, rect.right); }
        if (['hidden', 'clip'].includes(s.overflowY)) { top = Math.max(top, rect.top); bottom = Math.min(bottom, rect.bottom); }
        if (['auto', 'scroll'].includes(s.overflowX) && node !== document.body) scrolls = true;
      }
      if (s.position === 'fixed') break;
    }
    return { left, right, top, bottom, scrolls };
  };
  // Shown = rendered with a usable size, not transparent/invisible, and not fully cut off by a
  // clipping ancestor or the viewport's horizontal extent (off-canvas panels count as hidden).
  const shown = (el, rect, clip) => rect.width >= 2 && rect.height >= 2
    && el.checkVisibility({ opacityProperty: true, visibilityProperty: true })
    && Math.min(rect.right, clip.right) > Math.max(rect.left, clip.left, 0) && Math.min(rect.left, clip.right) < width
    && Math.min(rect.bottom, clip.bottom) > Math.max(rect.top, clip.top);

  const scrollWidth = document.documentElement.scrollWidth;
  if (scrollWidth > width) findings.push({ rule: 'horizontal-scroll', selector: 'html', name: '', area: '', box: { x: 0, y: 0, width: scrollWidth, height: 0 }, detail: `document scrollWidth ${scrollWidth}px > innerWidth ${width}px` });

  // Allowlist: inline links inside running text (WCAG 2.5.8 inline exception) and the
  // simulation-only banner, which real workspaces never show.
  const exempt = el => {
    if (el.closest('.company-simulation-banner')) return true;
    if (el.tagName !== 'A' || style(el).display !== 'inline') return false;
    const block = el.parentElement?.closest('p,li,td,th,blockquote,dd,figcaption,small,label,span');
    return Boolean(block) && clean(block.textContent).length > clean(el.textContent).length + 2;
  };
  // A form control's touch box includes its <label>s, which also activate it.
  const touchBox = el => {
    const rects = [el.getBoundingClientRect(), ...[...(el.labels ?? [])].map(label => label.getBoundingClientRect()).filter(r => r.width && r.height)];
    const left = Math.min(...rects.map(r => r.left)), top = Math.min(...rects.map(r => r.top));
    const right = Math.max(...rects.map(r => r.right)), bottom = Math.max(...rects.map(r => r.bottom));
    return { left, top, right, bottom, width: right - left, height: bottom - top };
  };
  const interactive = 'button, a[href], input:not([type=hidden]), select, textarea, [role=button], [role=switch], [role=tab]';
  for (const el of root.querySelectorAll(interactive)) {
    const rect = el.getBoundingClientRect();
    if (!shown(el, rect, clipping(el)) || exempt(el)) continue;
    const target = touchBox(el);
    if (target.width + 0.5 < minTarget || target.height + 0.5 < minTarget) report('touch-target', el, target, `${Math.round(target.width)}×${Math.round(target.height)} < ${minTarget}×${minTarget}`);
  }

  // Names the descendant reaching furthest right, which is usually what is being cut off.
  const widest = (el, rect) => {
    let best, right = rect.right;
    for (const child of [...el.querySelectorAll('*')].slice(0, 3000)) {
      const edge = child.getBoundingClientRect().right;
      if (edge > right + tolerance) { best = child; right = edge; }
    }
    return best ? `; widest: ${selectorOf(best)} "${nameOf(best).slice(0, 40)}" reaches ${Math.round(right)}px` : '';
  };
  const reported = new Set();
  for (const el of root.querySelectorAll('*')) {
    if (el.closest('svg') && el.tagName.toLowerCase() !== 'svg') continue;
    const rect = el.getBoundingClientRect(), clip = clipping(el), s = style(el);
    if (!shown(el, rect, clip)) continue;
    // Clipped text: the element hides overflowing content without an ellipsis or a tooltip.
    if (['hidden', 'clip'].includes(s.overflowX) && el.scrollWidth > el.clientWidth + tolerance && el.clientWidth > 0
      && s.textOverflow !== 'ellipsis' && !el.closest('[title]') && !['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName)
      && clean(el.textContent)) report('clipped-text', el, rect, `scrollWidth ${el.scrollWidth}px > clientWidth ${el.clientWidth}px${widest(el, rect)}`);
    // Past the right edge: visibly cut off by the viewport, not by an ancestor or a local scroller.
    if (rect.right > width + tolerance && rect.left < width && !clip.scrolls && Math.min(rect.right, clip.right) > width + tolerance) {
      let parent = el.parentElement, nested = false;
      for (; parent && !nested; parent = parent.parentElement) nested = reported.has(parent);
      if (!nested) { reported.add(el); report('past-right-edge', el, rect, `right edge ${Math.round(rect.right)}px > viewport ${width}px`); }
    }
  }
  return { width, height: window.innerHeight, scrollWidth, findings };
}
