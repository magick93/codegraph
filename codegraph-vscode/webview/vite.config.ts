import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  build: {
    outDir: '../dist/webview',
    lib: {
      entry: './src/main.ts',
      formats: ['iife'],
      name: 'IfmlDiagram',
    },
    rollupOptions: {
      output: {
        entryFileNames: 'ifml-diagram.js',
        assetFileNames: 'ifml-diagram.css',
      },
    },
  },
});
