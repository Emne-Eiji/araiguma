/// <reference types="vitest/config" />
import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

// The repository root (fixture JSON files are imported from there and must
// be servable by the dev server).
const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5199,
    strictPort: true,
    host: '127.0.0.1',
    fs: { allow: [repoRoot] },
  },
  preview: {
    port: 5199,
    strictPort: true,
    host: '127.0.0.1',
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts'],
  },
});
