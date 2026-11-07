import { test, expect, type Page } from '@playwright/test'

/**
 * Models page and the "Add models" catalog, in demo mode. The mock reports a
 * 16 GB Apple Silicon Mac (get_system_memory in src/lib/mockTauri.ts), so the
 * suggestions are the chat models that fit in ~10.7 GB.
 */
async function gotoModels(page: Page, query = '') {
  await page.goto(`/models?demo=true&phase=serving${query}`)
  await page.waitForLoadState('domcontentloaded')
}

test.describe('Models (demo mode)', () => {
  test("lists the organization's models", async ({ page }) => {
    await gotoModels(page)

    await expect(page.getByTestId('model-row')).toHaveCount(2)
    await expect(page.getByText('Llama 3.2 3B Instruct')).toBeVisible()
    await expect(page.getByText('2 devices')).toBeVisible()
    await expect(page.getByText('Disabled', { exact: true })).toBeVisible()
  })

  test('an admin adds a recommended model', async ({ page }) => {
    await gotoModels(page)
    await page.getByRole('button', { name: 'Add models' }).click()

    const dialog = page.getByRole('dialog')
    const recommended = dialog.getByRole('region', { name: 'Recommended for this device' })
    // The 32B and 70B models don't fit a 16 GB Mac; models already in the
    // organization aren't suggested again.
    await expect(recommended.getByTestId('catalog-row')).toHaveCount(3)
    await expect(recommended).not.toContainText('32B')
    await expect(recommended).not.toContainText('Llama 3.2 3B')

    await recommended.getByRole('button', { name: 'Add Meta Llama 3.1 8B Instruct' }).click()
    await expect(recommended.getByText('Added')).toBeVisible()

    await page.keyboard.press('Escape')
    await expect(page.getByTestId('model-row')).toHaveCount(3)
  })

  test('the catalog can be searched', async ({ page }) => {
    await gotoModels(page)
    await page.getByRole('button', { name: 'Add models' }).click()

    const dialog = page.getByRole('dialog')
    await dialog.getByLabel('Search models').fill('gemma')
    await expect(dialog.getByRole('region', { name: 'Recommended for this device' })).toHaveCount(0)
    await expect(dialog.getByTestId('catalog-row')).toHaveCount(1)
    await expect(dialog.getByText('gemma 3 4b it')).toBeVisible()
  })

  test('an admin disables and removes a model', async ({ page }) => {
    await gotoModels(page)

    const toggle = page.getByRole('switch', { name: 'Disable Llama 3.2 3B Instruct' })
    await toggle.click()
    await expect(page.getByRole('switch', { name: 'Enable Llama 3.2 3B Instruct' })).toHaveAttribute('aria-checked', 'false')

    await page.getByRole('button', { name: 'Remove Llama 3.2 3B Instruct' }).click()
    await page.getByRole('dialog').getByRole('button', { name: 'Remove model' }).click()
    await expect(page.getByTestId('model-row')).toHaveCount(1)
  })

  test('Hugging Face being down is explained, with a retry', async ({ page }) => {
    await gotoModels(page, '&catalog=down')
    await page.getByRole('button', { name: 'Add models' }).click()

    const dialog = page.getByRole('dialog')
    await expect(dialog.getByRole('alert')).toContainText('Hugging Face is unreachable')
    await expect(dialog.getByRole('button', { name: 'Try again' })).toBeVisible()
  })

  test('a member sees the models read-only', async ({ page }) => {
    await gotoModels(page, '&role=standard')

    await expect(page.getByTestId('model-row')).toHaveCount(2)
    await expect(page.getByText('Only admins can add or remove models.')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Add models' })).toHaveCount(0)
    await expect(page.getByRole('switch')).toHaveCount(0)
  })
})
