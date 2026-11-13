import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import path from 'path'

// Node 25+ ships its own localStorage global, which shadows jsdom's. Older
// Node (CI runs 20) has no such flag and would refuse it.
const nodeMajor = Number(process.versions.node.split('.')[0])
const execArgv = nodeMajor >= 25 ? ['--no-experimental-webstorage'] : []

// Unit tests only (src/**/*.test.ts[x]); the Playwright specs in tests/ run
// against the dev server with `npm test`.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.{ts,tsx}'],
    restoreMocks: true,
    unstubEnvs: true,
    poolOptions: { forks: { execArgv } },
  },
})
