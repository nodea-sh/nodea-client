import { test, expect, type Page } from '@playwright/test'

/**
 * Team page, in demo mode (src/lib/mockApi.ts: five members - the demo user
 * among them - and two pending invitations). Same rules as the web console's
 * Team page: everyone sees the members; admins invite, change roles and
 * revoke invitations.
 */
async function gotoTeam(page: Page, query = '') {
  await page.goto(`/team?demo=true&phase=serving${query}`)
  await page.waitForLoadState('domcontentloaded')
}

test.describe('Team (demo mode)', () => {
  test('lists the members with their role, devices and plan usage', async ({ page }) => {
    await gotoTeam(page)

    await expect(page.getByRole('heading', { name: '5 members' })).toBeVisible()
    await expect(page.getByTestId('member-row')).toHaveCount(5)
    await expect(page.getByTestId('plan-usage')).toContainText('Starter plan')
    await expect(page.getByTestId('plan-usage')).toContainText('2 of 5 devices connected')
    await expect(page.getByTestId('member-row').filter({ hasText: 'Diana Prince' })).toContainText('Deactivated')
  })

  test('an admin invites a teammate', async ({ page }) => {
    await gotoTeam(page)

    await expect(page.getByTestId('invitation-row')).toHaveCount(2)
    await page.getByPlaceholder('teammate@company.com').fill('grace@example.com')
    await page.getByLabel('Role', { exact: true }).selectOption('admin')
    await page.getByRole('button', { name: 'Send invite' }).click()

    await expect(page.getByText('Invitation sent to grace@example.com.')).toBeVisible()
    await expect(page.getByTestId('invitation-row')).toHaveCount(3)
    await expect(page.getByTestId('invitation-row').filter({ hasText: 'grace@example.com' })).toContainText('Admin')
  })

  test('an admin revokes a pending invitation', async ({ page }) => {
    await gotoTeam(page)

    await page.getByRole('button', { name: 'Revoke invitation to emily@example.com' }).click()
    await expect(page.getByText('Invitation to emily@example.com revoked.')).toBeVisible()
    await expect(page.getByTestId('invitation-row')).toHaveCount(1)
  })

  test("an admin changes a member's role, but not their own", async ({ page }) => {
    await gotoTeam(page)

    await expect(page.getByRole('button', { name: 'Edit role for Demo User' })).toHaveCount(0)
    // Deactivated members can't be edited either.
    await expect(page.getByRole('button', { name: 'Edit role for Diana Prince' })).toHaveCount(0)

    await page.getByRole('button', { name: 'Edit role for Bob Smith' }).click()
    const dialog = page.getByRole('dialog')
    const save = dialog.getByRole('button', { name: 'Save' })
    await expect(save).toBeDisabled()
    await dialog.getByLabel('Role for Bob Smith').selectOption('admin')
    await save.click()

    await expect(page.getByText('Bob Smith is now an Admin.')).toBeVisible()
    await expect(page.getByTestId('member-row').filter({ hasText: 'Bob Smith' })).toContainText('Admin')
  })

  test('a member sees the team but no invite, role or invitation controls', async ({ page }) => {
    await gotoTeam(page, '&role=standard')

    await expect(page.getByTestId('member-row')).toHaveCount(5)
    await expect(page.getByRole('heading', { name: 'Invite a teammate' })).toHaveCount(0)
    await expect(page.getByRole('heading', { name: 'Pending invitations' })).toHaveCount(0)
    await expect(page.getByRole('button', { name: /^Edit role/ })).toHaveCount(0)
  })

  test('the sidebar links to Team', async ({ page }) => {
    await page.goto('/node?demo=true&phase=serving')
    await page.getByRole('navigation').getByRole('link', { name: 'Team' }).click()
    await expect(page).toHaveURL(/\/team/)
  })
})
