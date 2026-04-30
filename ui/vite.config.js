/// <reference types="vite/client" />
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import { defineConfig } from 'vite';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// pnpm hoists packages into node_modules/.pnpm/<pkg>@<version>/node_modules/<pkg>.
// Vite's default fs.allow list does not include that virtual store path, which
// causes "outside of Vite serving allow list" errors for packages like
// monaco-editor that load CSS/fonts (codicon.ttf, codicon-modifiers.css) via
// runtime URLs. Allowing the entire monorepo root covers both ui/node_modules
// symlinks and their real targets under <repo>/node_modules/.pnpm/*.
export default defineConfig({
  server: {
    fs: {
      allow: [
        // Monorepo root — covers ui/node_modules and the pnpm virtual
        // store at <repo>/node_modules/.pnpm/*
        resolve(__dirname, '..'),
      ],
    },
  },
});
