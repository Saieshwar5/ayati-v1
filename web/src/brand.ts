import './brand.css';

/** Shared identity artwork; CSS masks keep the same geometry in both themes. */
export function brand(className = '', symbolOnly = false): HTMLSpanElement {
  const node = document.createElement('span');
  node.className = `relay-brand ${symbolOnly ? 'relay-symbol' : 'relay-logo'} ${className}`.trim();
  node.setAttribute('role', 'img');
  node.setAttribute('aria-label', 'Relay');
  return node;
}
