import { defineConfig } from 'vite';

export default defineConfig({
  base: './',
  build: {
    outDir: '../server/dist',
    emptyOutDir: true,
    target: 'es2022',
    chunkSizeWarningLimit: 1500
  }
});
