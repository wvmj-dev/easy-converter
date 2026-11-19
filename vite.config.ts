import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

// Tauri expects a fixed dev server port and a local-only host.
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: 'localhost',
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    target: 'es2022',
  },
  test: {
    environment: 'jsdom',
  },
})
