import { test, expect } from '@playwright/test'

/**
 * In-app updates in demo mode. Rust owns the updater; src/lib/mockTauri.ts
 * serves its state, pinned with ?update=<downloading|ready|installing>, and
 * logs the commands the sidebar sends.
 */
test.describe('In-app updates', () => {
  test.beforeEach(async ({ page }) => {
    page.on('pageerror', (err) => console.log('[PAGE ERROR]', err.message))
  })

  test('nothing shows while there is no update, or while it downloads', async ({ page }) => {
    for (const query of ['', '&update=downloading']) {
      await page.goto(`/node?demo=true${query}`)
      await expect(page.getByText('Nodea').first()).toBeVisible()
      await expect(page.getByText('Update ready · Restart')).toHaveCount(0)
    }
  })

  test('a ready update shows in the sidebar footer and Restart installs it', async ({ page }) => {
    await page.goto('/node?demo=true&update=ready')

    const restarted = page.waitForEvent('console', (msg) =>
      msg.text().includes('[Mock Tauri] restart_to_update'),
    )
    await page.getByRole('button', { name: 'Update ready · Restart' }).click()
    await restarted
  })

  test('the ready item can be dismissed', async ({ page }) => {
    await page.goto('/node?demo=true&update=ready')
    await expect(page.getByRole('button', { name: 'Update ready · Restart' })).toBeVisible()

    await page.getByRole('button', { name: 'Dismiss update' }).click()

    await expect(page.getByText('Update ready · Restart')).toHaveCount(0)
  })

  test('restarting says it is finishing the current request', async ({ page }) => {
    await page.goto('/node?demo=true&update=installing')

    await expect(page.getByRole('status').filter({ hasText: 'Finishing current request…' })).toBeVisible()
  })

  test('Check for updates reports the result', async ({ page }) => {
    await page.goto('/node?demo=true')

    await page.getByRole('button', { name: /Demo User/ }).click()
    await page.getByRole('button', { name: 'Check for updates' }).click()

    await expect(page.getByText("You're up to date")).toBeVisible()
  })

  test('the first launch after an update says so, once', async ({ page }) => {
    await page.goto('/node?demo=true')
    // The sidebar stores the running version when it mounts; overwrite it
    // only after that, as if the previous launch had been 0.0.1.
    await expect(page.getByRole('button', { name: /Demo User/ })).toBeVisible()
    await expect.poll(() => page.evaluate(() => localStorage.getItem('nodea.lastVersion'))).not.toBeNull()
    await page.evaluate(() => localStorage.setItem('nodea.lastVersion', '0.0.1'))

    await page.reload()
    await expect(page.getByText(/^Updated to /)).toBeVisible()
    await expect(page.getByRole('button', { name: "What's new" })).toBeVisible()

    await page.reload()
    await expect(page.getByText('Nodea').first()).toBeVisible()
    await expect(page.getByText(/^Updated to /)).toHaveCount(0)
  })
})
