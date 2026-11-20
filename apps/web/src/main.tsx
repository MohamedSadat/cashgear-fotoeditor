import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './style.css';
import editorStyles from './style.css?inline';

// The canvas needs its layout before the worker sizes the backing buffer.
if (!getComputedStyle(document.documentElement).getPropertyValue('--editor-styles-ready').trim()) {
  const stylesheet = document.createElement('style');
  stylesheet.textContent = editorStyles;
  document.head.append(stylesheet);
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
