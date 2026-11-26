import type { Locator, Page } from 'playwright'
import { DEMO_BASE_URL } from '../demo-server'
import type { Shot } from '../shot'
import { openApp } from '../ui'

async function openAppSettings(page: Page): Promise<Locator> {
  await page.getByRole('button', { name: 'Settings', exact: true }).first().click()
  const dialog = page.getByRole('dialog', { name: 'Preferences' })
  await dialog.waitFor()
  await page.waitForTimeout(500)
  return dialog
}

async function openProjectSettings(page: Page): Promise<Locator> {
  await page.locator('[aria-label^="Project: acme-shop"]').first().click({ button: 'right' })
  await page.getByRole('menuitem', { name: 'Settings', exact: true }).click()
  const dialog = page.getByRole('dialog', { name: 'Project settings' })
  await dialog.waitFor()
  await page.waitForTimeout(800)
  return dialog
}

async function showCategory(page: Page, dialog: Locator, label: string): Promise<void> {
  await dialog
    .getByRole('navigation', { name: 'Settings pages' })
    .getByRole('button', { name: label, exact: true })
    .click()
  await page.waitForTimeout(800)
}

function settingsGroup(page: Page, title: string): Locator {
  return page
    .getByRole('heading', { name: title, exact: true })
    .locator('xpath=ancestor::section[1]')
}

export const shots: Shot[] = [
  {
    id: 'settings-app-preferences',
    viewport: { width: 1280, height: 920 },
    run: async (page) => {
      await openApp(page)
      await openAppSettings(page)
    }
  },
  {
    id: 'settings-behavior',
    viewport: { width: 1280, height: 920 },
    run: async (page) => {
      await openApp(page)
      const dialog = await openAppSettings(page)
      await showCategory(page, dialog, 'Preferences')
      return dialog
    }
  },
  {
    id: 'settings-shortcuts-conflict',
    run: async (page) => {
      await openApp(page)
      const dialog = await openAppSettings(page)
      await showCategory(page, dialog, 'Keyboard shortcuts')
      await dialog.getByRole('textbox', { name: 'Filter shortcuts' }).fill('Search in terminal')
      await page.waitForTimeout(300)
      await dialog.getByRole('button', { name: 'Record Search in terminal shortcut' }).click()
      await page.waitForTimeout(300)
      await page.keyboard.press('Control+B')
      await page.getByText('Use anyway').waitFor()
      await page.waitForTimeout(500)
      return dialog
    }
  },
  {
    id: 'settings-diagnostics',
    run: async (page) => {
      await openApp(page)
      const dialog = await openAppSettings(page)
      await showCategory(page, dialog, 'About')
      const logs = settingsGroup(page, 'Logs and bug reports')
      await logs.scrollIntoViewIfNeeded()
      await page.waitForTimeout(400)
      return logs
    }
  },
  {
    id: 'settings-project-general',
    run: async (page) => {
      await openApp(page)
      await openProjectSettings(page)
    }
  },
  {
    id: 'settings-project-env',
    run: async (page) => {
      await openApp(page)
      const dialog = await openProjectSettings(page)
      await showCategory(page, dialog, 'Environment variables')
      await dialog.getByRole('button', { name: 'Add variable' }).click()
      await dialog.getByPlaceholder('KEY').last().fill('STRIPE_MODE')
      await dialog.getByPlaceholder('value').last().fill('test')
      await page.waitForTimeout(400)
      return dialog
    }
  },
  {
    id: 'settings-project-symlinks',
    run: async (page) => {
      await openApp(page)
      const dialog = await openProjectSettings(page)
      await showCategory(page, dialog, 'Worktree symlinks')
      return dialog
    }
  },
  {
    id: 'settings-project-emergency',
    run: async (page) => {
      await openApp(page)
      const dialog = await openProjectSettings(page)
      await showCategory(page, dialog, 'Emergency Mode')
      return dialog.locator('[data-settings-section="emergency"]')
    }
  },
  {
    id: 'settings-theme-picker',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'Color themes' }).click()
      const picker = page.getByRole('dialog', { name: 'Color theme picker' })
      await picker.waitFor()
      await picker.getByRole('option', { name: 'Tokyo Night', exact: true }).hover()
      await page.waitForTimeout(600)
    }
  },
  {
    id: 'settings-shortcut-menu',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'Open keyboard shortcuts menu' }).click()
      const menu = page.getByRole('dialog', { name: 'Shortcut Menu' })
      await menu.waitFor()
      await page.waitForTimeout(400)
      return menu
    }
  },
  {
    id: 'settings-troubleshooting-token',
    run: async (page) => {
      await page.goto(`${DEMO_BASE_URL}/#/`)
      await page.getByRole('heading', { name: 'Server access token required' }).waitFor()
      await page.getByRole('textbox', { name: 'Access token' }).fill('not-the-right-token')
      await page.getByRole('button', { name: 'Continue' }).click()
      await page.getByRole('alert').waitFor()
      await page.waitForTimeout(300)
      return page.getByRole('form', { name: 'Server access token' })
    }
  },
  // Keep last: the chosen theme is saved on the demo server, so later shots would turn light.
  {
    id: 'settings-light-theme',
    run: async (page) => {
      await openApp(page)
      await page.getByRole('button', { name: 'Color themes' }).click()
      const picker = page.getByRole('dialog', { name: 'Color theme picker' })
      await picker.waitFor()
      await picker.getByRole('option', { name: 'GitHub Light', exact: true }).click()
      await picker.waitFor({ state: 'detached' })
      await page.waitForTimeout(300)
      // Closing the picker puts focus back on the rail button, which shows its tooltip.
      await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur())
      await page.mouse.move(1000, 400)
      await page.waitForTimeout(800)
    }
  }
]
