import { test, expect, type Page } from '@playwright/test'

/**
 * API keys page, in demo mode (`?demo=true`, see src/lib/mockApi.ts). Every
 * member sees the organization's keys, masked; only admins create and revoke.
 */
async function gotoKeys(page: Page, query = '') {
  await page.goto(`/api-keys?demo=true${query}`)
  await page.waitForLoadState('domcontentloaded')
}

test.describe('API keys (demo mode)', () => {
  test('lists the keys masked, with the disabled one explained', async ({ page }) => {
    await gotoKeys(page)

    await expect(page.getByTestId('api-key-row')).toHaveCount(3)
    await expect(page.getByText('Xk3...9fQa')).toBeVisible()
    await expect(page.getByText('Disabled: its creator left the organization')).toBeVisible()
  })

  test('an admin creates a key and sees the secret once', async ({ page }) => {
    await gotoKeys(page)

    await page.getByRole('button', { name: 'Create key' }).click()
    const dialog = page.getByRole('dialog')
    const submit = dialog.getByRole('button', { name: 'Create key' })
    await expect(submit).toBeDisabled()
    await dialog.getByLabel('Name').fill('CI')
    await submit.click()

    await expect(dialog).toContainText("It won't be shown again.")
    const secret = await dialog.getByTestId('new-api-key').textContent()
    expect(secret?.length).toBeGreaterThan(10)

    // Escape doesn't lose the secret; only the deliberate button closes it.
    await page.keyboard.press('Escape')
    await expect(dialog).toBeVisible()
    await dialog.getByRole('button', { name: "I've saved it" }).click()
    await expect(dialog).toBeHidden()

    // The list shows the new key masked, never the secret.
    await expect(page.getByTestId('api-key-row')).toHaveCount(4)
    await expect(page.getByText(secret!)).toHaveCount(0)
  })

  test('a duplicate name is explained in the dialog', async ({ page }) => {
    await gotoKeys(page)

    await page.getByRole('button', { name: 'Create key' }).click()
    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Name').fill('Support bot')
    await dialog.getByRole('button', { name: 'Create key' }).click()

    await expect(dialog.getByRole('alert')).toContainText('already exists')
  })

  test('revoking asks first, naming the key', async ({ page }) => {
    await gotoKeys(page)

    await page.getByRole('button', { name: 'Revoke Support bot' }).click()
    const dialog = page.getByRole('dialog')
    await expect(dialog).toContainText('Revoke "Support bot"?')
    await dialog.getByRole('button', { name: 'Revoke key' }).click()

    await expect(dialog).toBeHidden()
    await expect(page.getByTestId('api-key-row')).toHaveCount(2)
  })

  test('a member sees the keys but cannot create or revoke them', async ({ page }) => {
    await gotoKeys(page, '&role=standard')

    await expect(page.getByTestId('api-key-row')).toHaveCount(3)
    await expect(page.getByText('Only admins can create or revoke keys.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Create key' })).toHaveCount(0)
    await expect(page.getByRole('button', { name: /^Revoke/ })).toHaveCount(0)
  })

  test('a new organization gets an empty state that offers the next step', async ({ page }) => {
    await gotoKeys(page, '&setup=fresh')

    await expect(page.getByText('No API keys yet')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Create key' })).toBeVisible()
  })

  test('the sidebar links to API keys', async ({ page }) => {
    await page.goto('/node?demo=true&phase=serving')
    await page.getByRole('navigation').getByRole('link', { name: 'API keys' }).click()
    await expect(page).toHaveURL(/\/api-keys/)
  })
})
