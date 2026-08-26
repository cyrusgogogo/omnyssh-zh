import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';
import { resolve } from 'node:path';

// Fixed dev port matches tauri.conf.json devUrl so `cargo tauri dev` finds it.
export default defineConfig({
  plugins: [sveltekit()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: [resolve(__dirname, '../../..')] }
  },
  build: { target: 'esnext' }
});
