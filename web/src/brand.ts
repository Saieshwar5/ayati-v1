import './brand.css';

/** Shared identity artwork; CSS masks keep the same geometry in both themes. */
export function brand(className = '', symbolOnly = false): HTMLSpanElement {
  const node = document.createElement('span');
  node.className = `cuckoo-brand ${symbolOnly ? 'cuckoo-symbol' : 'cuckoo-logo'} ${className}`.trim();
  node.setAttribute('role', 'img');
  node.setAttribute('aria-label', 'Cuckoo');
  return node;
}
