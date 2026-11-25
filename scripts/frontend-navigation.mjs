export async function selectCategorySource(page, name) {
  await page.getByRole("navigation", { name: "主导航" }).getByRole("button", { name: "分类", exact: true }).click();
  await page.getByRole("navigation", { name: "分类片源" }).getByRole("button", { name, exact: true }).click();
}
export async function openRecommendations(page, name) {
  await selectCategorySource(page, name);
  await page.getByRole("button", { name: `推荐分区 · ${name}`, exact: true }).click();
}
export const installedTab = page => page.getByRole("navigation", { name: "插件中心分栏" }).getByRole("button", { name: /^已安装/ }).click();
export const importTab = page => page.getByRole("navigation", { name: "插件中心分栏" }).getByRole("button", { name: /^导入与(列表)?订阅$/ }).click();

// The restored player has a single dock, not the old tabbed panel heading.
// Use visible controls and the current More > Quality and Lines navigation.
async function closePlayerPanel(page) {
  const close = page.locator('.legacy-panel-heading').getByRole('button', { name: '关闭面板', exact: true });
  if (await close.isVisible()) await close.click();
}
async function wakePlayerControls(page) {
  // Move the real pointer over the player surface; error overlays can legitimately
  // cover the video. Do not force clicks through overlays or dismiss errors.
  const surface = page.locator('.hg-player-ui.player-root').first();
  if (await surface.isVisible()) {
    const bounds = await surface.boundingBox();
    if (bounds) await page.mouse.move(bounds.x + bounds.width * 0.2, bounds.y + bounds.height * 0.25);
  }
}
export async function playerPanel(page, panel) {
  const title = panel === '播放设置' ? '画质与线路' : panel === '详情' ? '作品详情' : panel;
  const heading = page.locator('.legacy-panel-heading').getByRole('heading', { name: title, exact: true });
  if (await heading.isVisible()) return;
  if (panel === '详情' && await page.getByRole('dialog', { name: '作品详情' }).isVisible()) return;
  await closePlayerPanel(page);
  await wakePlayerControls(page);
  if (panel === '播放设置') {
    await page.getByRole('button', { name: '更多', exact: true }).click();
    await page.getByRole('button', { name: '画质与线路', exact: true }).click();
    await heading.waitFor();
  } else if (panel === '详情') {
    // In full playback the detail action is inside More; preview has its own action.
    const direct = page.getByRole('button', { name: '作品详情', exact: true });
    if (!(await direct.isVisible())) {
      await page.getByRole('button', { name: '更多', exact: true }).click();
    }
    await direct.click();
    await page.getByRole('dialog', { name: '作品详情' }).waitFor();
  } else {
    await page.getByRole('button', { name: panel, exact: true }).click();
    await heading.waitFor();
  }
}
export async function chooseEpisode(page, name) {
  const detail = page.getByRole('dialog', { name: '作品详情' });
  if (await detail.isVisible()) {
    await detail.getByRole('button', {name, exact: true}).click();
  } else {
    await playerPanel(page, '选集');
    await page.locator('.hg-episode-picker').getByRole('button', {name, exact: true}).click();
  }
  await playerPanel(page, '播放设置');
}
