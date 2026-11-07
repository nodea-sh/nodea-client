import { test, expect } from '@playwright/test'

/**
 * Settings page in demo mode (`?demo=true`): AuthContext auto-authenticates a
 * demo user and `get_model_cache_info` is served by src/lib/mockTauri.ts, so
 * the page renders exactly as it does signed in - with no Supabase project,
 * no backend and no Tauri host.
 */
test.describe('Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    page.on('pageerror', (err) => console.log('[PAGE ERROR]', err.message))

    await page.goto('/settings?demo=true')
    await page.waitForLoadState('domcontentloaded')
  })

  test('should display the Settings page', async ({ page }) => {
    await expect(page.locator('h1:has-text("Settings")')).toBeVisible()
  })

  test('should show Share compute and Model cache rows', async ({ page }) => {
    await expect(page.getByText('Share compute')).toBeVisible()
    await expect(page.getByRole('switch')).toBeVisible()

    await expect(page.getByText('Model cache')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Reveal' })).toBeVisible()
  })

  test('Reveal asks the app to open its own cache folder', async ({ page }) => {
    // The webview never hands a path to shell.open (its scope allows only the
    // web console); the Rust command opens the cache dir it already knows.
    const revealed = page.waitForEvent('console', (msg) =>
      msg.text().includes('[Mock Tauri] reveal_model_cache'),
    )
    await page.getByRole('button', { name: 'Reveal' }).click()
    await revealed
  })

  test('should not show settings with no backend support', async ({ page }) => {
    // Ceiling / pause-on-battery / launch-at-login have no backend yet -
    // the row is hidden rather than shipped as a dead control.
    await expect(page.getByText('Ceiling')).toHaveCount(0)
    await expect(page.getByText('Pause on battery')).toHaveCount(0)
    await expect(page.getByText('Launch at login')).toHaveCount(0)
  })

  test('shows this node\'s name and renames it', async ({ page }) => {
    // The demo device (get_device_id -> device-001) is called "MacBook Pro".
    await expect(page.getByText('Node name')).toBeVisible()
    await expect(page.getByText('MacBook Pro', { exact: true })).toBeVisible()

    await page.getByRole('button', { name: 'Rename' }).click()
    await page.getByLabel('Node name').fill('  Studio Mac  ')
    await page.getByRole('button', { name: 'Save' }).click()

    // Trimmed, and the edit field is gone.
    await expect(page.getByText('Studio Mac', { exact: true })).toBeVisible()
    await expect(page.getByLabel('Node name')).toHaveCount(0)
    await expect(page.getByRole('button', { name: 'Rename' })).toBeVisible()
  })

  test('Enter saves and Escape cancels a rename', async ({ page }) => {
    await page.getByRole('button', { name: 'Rename' }).click()
    await page.getByLabel('Node name').fill('Discarded')
    await page.getByLabel('Node name').press('Escape')
    await expect(page.getByText('MacBook Pro', { exact: true })).toBeVisible()

    await page.getByRole('button', { name: 'Rename' }).click()
    await page.getByLabel('Node name').fill('Desk Mac')
    await page.getByLabel('Node name').press('Enter')
    await expect(page.getByText('Desk Mac', { exact: true })).toBeVisible()
  })

  test('an empty name is rejected and the old one kept', async ({ page }) => {
    await page.getByRole('button', { name: 'Rename' }).click()
    await page.getByLabel('Node name').fill('   ')
    await page.getByRole('button', { name: 'Save' }).click()

    await expect(page.getByRole('alert')).toHaveText('Enter a name.')
    await page.getByRole('button', { name: 'Cancel' }).click()
    await expect(page.getByText('MacBook Pro', { exact: true })).toBeVisible()
  })

  test('a node that never connected cannot be renamed yet', async ({ page }) => {
    await page.goto('/settings?demo=true&device=new')

    await expect(page.getByText('Connect once to name this node.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Rename' })).toHaveCount(0)
  })

  test('should show app version and a sign out control', async ({ page }) => {
    await expect(page.getByText(/^Nodea \d/)).toBeVisible()
    await expect(page.getByRole('button', { name: 'Sign out' })).toBeVisible()
  })
})
