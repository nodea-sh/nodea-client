import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig(({ command, mode }) => {
  const env = loadEnv(mode, __dirname, '')

  if (command === 'build' && mode === 'production' && !env.VITE_API_BASE_URL) {
    throw new Error(
      'VITE_API_BASE_URL must be set for production builds (see tauri/.env.production).'
    )
  }

  // The app only ever talks to the backend through its public name
  // (api.getnodea.com), never the hosting provider's - it ends up in the
  // API examples users copy.
  if (env.VITE_API_BASE_URL?.includes('fly.dev')) {
    throw new Error(
      `VITE_API_BASE_URL is ${env.VITE_API_BASE_URL}: use https://api.getnodea.com/site instead.`
    )
  }

  return {
    plugins: [react()],
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
    },
    envPrefix: ['VITE_', 'TAURI_'],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    build: {
      target: ['es2021', 'chrome100', 'safari13'],
      minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
      sourcemap: !!process.env.TAURI_DEBUG,
    },
  }
})
