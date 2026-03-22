import { test, expect } from './fixtures';

test.describe.serial('Review workflow', () => {
  let sessionId: string;
  let threadId: string;

  test('shows empty state before session creation', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.getByText('No active review sessions')).toBeVisible();

    // Create session for all subsequent tests
    sessionId = await lgtm.createSession();
  });

  test('shows tab after session creation', async ({ page, lgtm }) => {
    // Session was created in previous test via API; Shell polls every 2s
    sessionId = sessionId || await lgtm.createSession();
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.tab')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.tab')).toContainText('feature');
  });

  test('shows diff when clicking file', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    // Wait for session tab to load, then file tree
    await expect(page.locator('.tab')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.file-item')).not.toHaveCount(0, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.locator('.diff-line')).not.toHaveCount(0, { timeout: 5000 });
  });

  test('adds a comment', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).not.toHaveCount(0, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.locator('.diff-line')).not.toHaveCount(0, { timeout: 5000 });

    // Click on a gutter of an "add" line to open comment form
    await page.locator('.diff-line.add .new-gutter').first().click();
    await expect(page.locator('.new-comment')).toBeVisible();

    // Type and submit
    await page.locator('.new-comment textarea').fill('Please fix this');
    await page.locator('.new-comment .btn-submit').click();

    // Thread should appear
    await expect(page.locator('.thread')).toBeVisible({ timeout: 5000 });
    await expect(page.locator('.thread')).toContainText('Please fix this');

    // Capture thread ID for later tests
    const resp = await fetch(`${lgtm.baseURL}/api/sessions/${sessionId}`);
    const session: any = await resp.json();
    threadId = session.threads[0].id;
  });

  test('submits to agent', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    const submitBtn = page.locator('button', { hasText: 'Submit to agent' });
    await expect(submitBtn).toBeVisible({ timeout: 5000 });
    await submitBtn.click();
    await expect(page.getByText('Waiting for agent...')).toBeVisible({ timeout: 5000 });
  });

  test('shows agent reply', async ({ page, lgtm }) => {
    await lgtm.agentReply(sessionId, threadId, 'Fixed the issue');

    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).not.toHaveCount(0, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.getByText('Fixed the issue')).toBeVisible({ timeout: 5000 });
  });

  test('resolves thread', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);
    await expect(page.locator('.file-item')).not.toHaveCount(0, { timeout: 5000 });
    await page.locator('.file-item').first().click();
    await expect(page.locator('.thread')).toBeVisible({ timeout: 5000 });

    const resolveBtn = page.locator('button', { hasText: 'Resolve' });
    await resolveBtn.click();
  });

  test('approves session', async ({ page, lgtm }) => {
    await page.goto(lgtm.baseURL);

    // Mark all files as reviewed
    const fileItems = page.locator('.file-item');
    await expect(fileItems.first()).toBeVisible({ timeout: 5000 });
    const count = await fileItems.count();
    for (let i = 0; i < count; i++) {
      const checkbox = fileItems.nth(i).locator('input[type="checkbox"]');
      if (!(await checkbox.isChecked())) {
        await checkbox.click();
        await page.waitForTimeout(300);
      }
    }
    // Wait for status bar to update
    await expect(page.getByText(`${count}/${count} files reviewed`)).toBeVisible({ timeout: 5000 });

    const approveBtn = page.locator('button', { hasText: 'Approve session' });
    await expect(approveBtn).toBeEnabled({ timeout: 10000 });
    await approveBtn.click();
    await expect(page.getByText('Session approved')).toBeVisible({ timeout: 5000 });
  });
});
