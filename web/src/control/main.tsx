import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { ControlApp } from './app';
import './control.css';

// Cuckoo Control is its own page (control.html): no workspace shell or legacy
// screens load here, and the workspace client never loads the console.
createRoot(document.querySelector<HTMLDivElement>('#root')!).render(<StrictMode><ControlApp /></StrictMode>);
