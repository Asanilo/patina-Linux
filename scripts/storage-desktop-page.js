// Injected only by the opt-in native storage acceptance worker.
(async () => {
  if (window.__storageAcceptanceRunning) return;
  window.__storageAcceptanceRunning = true;
  const { stage, root } = window.__storageAcceptance || {};
  const stages = ['move-data', 'restore-data', 'move-webview', 'cache', 'verify-cache'];
  const errors = [];
  const checks = [];
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (value, label) => { if (!value) throw new Error(label); checks.push(label); };
  const wait = async (test, label) => {
    const deadline = Date.now() + 45000;
    while (Date.now() < deadline) {
      const value = await test();
      if (value) return value;
      await delay(100);
    }
    throw new Error(`Timed out: ${label}`);
  };
  const invoke = async (command, args) => {
    let timer;
    try {
      return await Promise.race([
        window.__TAURI_INTERNALS__.invoke(command, args),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`IPC timeout: ${command}`)), 30000); }),
      ]);
    } finally { clearTimeout(timer); }
  };
  const snapshot = () => invoke('cmd_get_storage_snapshot');
  const compact = value => value && ({
    paths: value.paths,
    pendingMigration: value.pendingMigration,
    webviewCache: { path: value.webviewCache.path, clearOnRestart: value.webviewCache.clearOnRestart },
    maintenance: value.maintenance,
  });
  const report = async value => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch('/__storage_acceptance', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(value), signal: controller.signal,
      });
      if (!response.ok) throw new Error(`Report rejected: ${response.status}`);
    } finally { clearTimeout(timer); }
  };
  window.addEventListener('error', event => errors.push(String(event.message)));
  window.addEventListener('unhandledrejection', event => errors.push(String(event.reason)));
  const labels = {
    settings: ['Settings', '设置'],
    clear: ['Clear on restart', '重启时清理'],
    restart: ['Restart now', '立即重启', 'Restart and migrate', '重启并迁移'],
  };
  const labelledButton = names => [...document.querySelectorAll('button[aria-label]')]
    .find(element => names.includes(element.getAttribute('aria-label')));
  let before;
  let after;
  let restart;
  try {
    assert(stages.includes(stage) && /^\/tmp\/patina-storage-test-[^/]+$/.test(root), 'isolated stage configuration');
    await wait(() => window.__TAURI_INTERNALS__?.invoke, 'native Tauri IPC');
    const nav = await wait(() => labelledButton(labels.settings), 'Settings navigation');
    nav.click();
    const cacheSwitch = await wait(() => {
      const element = labelledButton(labels.clear);
      return element?.getAttribute('role') === 'switch' && !element.disabled ? element : null;
    }, 'loaded storage settings');
    let panel = cacheSwitch.closest('.qp-subpanel');
    assert(panel !== null, 'real storage panel rendered');
    cacheSwitch.scrollIntoView({ block: 'center' });
    before = await snapshot();
    assert(!before.maintenance.lastError, 'no startup maintenance error');
    assert(before.pendingMigration === null, 'previous appointment completed');
    const defaultData = `${root}/data/Patina`;
    assert(before.paths.defaultDataRoot === defaultData, 'production profile uses private data root');
    const sentinelKey = 'patina.storageAcceptance.sentinel';
    if (stage === 'move-data') {
      localStorage.setItem(sentinelKey, root);
    }
    assert(localStorage.getItem(sentinelKey) === root, 'WebView localStorage survives maintenance');
    if (stage === 'move-data') {
      assert(before.paths.dataRoot === defaultData, 'initial data root is default');
      const cancelled = { kind: 'data', selectedParent: `${root}/cancelled` };
      const preview = await invoke('cmd_preview_storage_migration', cancelled);
      assert(preview.currentDataRoot === defaultData && preview.targetDataRoot === `${root}/cancelled/Patina`, 'cancellable migration preview');
      await invoke('cmd_schedule_storage_migration', cancelled);
      const pending = await snapshot();
      assert(pending.pendingMigration?.targetDataRoot === preview.targetDataRoot, 'first migration appointment persisted');
      const cancelledSnapshot = await invoke('cmd_cancel_pending_storage_migration');
      assert(cancelledSnapshot.pendingMigration === null && (await snapshot()).pendingMigration === null, 'cancellation persisted');
      const move = { kind: 'data', selectedParent: `${root}/moved-data` };
      const movePreview = await invoke('cmd_preview_storage_migration', move);
      assert(movePreview.targetDataRoot === `${root}/moved-data/Patina`, 'data migration preview target');
      await invoke('cmd_schedule_storage_migration', move);
      after = await snapshot();
      assert(after.paths.dataRoot === defaultData && after.pendingMigration?.targetDataRoot === movePreview.targetDataRoot, 'appointment keeps live source until restart');
    } else if (stage === 'restore-data') {
      assert(before.paths.dataRoot === `${root}/moved-data/Patina` && before.paths.isCustomDataRoot, 'data migration completed');
      const preview = await invoke('cmd_preview_restore_default_storage', { kind: 'data' });
      assert(preview.targetDataRoot === defaultData, 'restore default preview');
      await invoke('cmd_schedule_restore_default_storage', { kind: 'data' });
      after = await snapshot();
      assert(after.paths.dataRoot === before.paths.dataRoot && after.pendingMigration?.targetDataRoot === defaultData, 'restore default appointment persisted');
    } else if (stage === 'move-webview') {
      assert(before.paths.dataRoot === defaultData && !before.paths.isCustomDataRoot, 'default data location restored');
      const move = { kind: 'webview', selectedParent: `${root}/moved-webview` };
      const preview = await invoke('cmd_preview_storage_migration', move);
      assert(preview.targetWebviewRoot.startsWith(`${root}/moved-webview/`), 'WebView migration preview target');
      localStorage.setItem('patina.storageAcceptance.webviewTarget', preview.targetWebviewRoot);
      await invoke('cmd_schedule_storage_migration', move);
      after = await snapshot();
      assert(after.paths.webviewRoot === before.paths.webviewRoot && after.pendingMigration?.targetWebviewRoot === preview.targetWebviewRoot, 'WebView appointment keeps live source');
    } else if (stage === 'cache') {
      assert(before.paths.webviewRoot === localStorage.getItem('patina.storageAcceptance.webviewTarget')
        && before.paths.isCustomWebviewRoot, 'WebView migration completed with persisted interface state');
      assert(!before.webviewCache.clearOnRestart && cacheSwitch.getAttribute('aria-checked') === 'false', 'cache clear initially disabled');
      cacheSwitch.click();
      await wait(() => cacheSwitch.getAttribute('aria-checked') === 'true', 'cache switch update');
      after = await snapshot();
      assert(after.webviewCache.clearOnRestart && after.pendingMigration === null, 'cache-only appointment persisted');
      restart = await wait(() => labelledButton(labels.restart), 'cache-only Restart now action');
      restart.scrollIntoView({ block: 'center' });
      await delay(100);
      assert(!restart.disabled && restart.getClientRects().length > 0, 'cache-only restart action visible and enabled');
    } else {
      after = await snapshot();
      assert(after.paths.dataRoot === defaultData && after.paths.webviewRoot === localStorage.getItem('patina.storageAcceptance.webviewTarget'), 'final data and WebView paths retained');
      assert(!after.webviewCache.clearOnRestart && after.maintenance.lastWebviewCacheClearAtMs > 0, 'cache-only maintenance completed');
      assert(cacheSwitch.getAttribute('aria-checked') === 'false', 'cache toggle reset in real Settings');
    }
    assert(!after.maintenance.lastError, 'no resulting maintenance error');
    if (after.pendingMigration) {
      // Appointments above deliberately use real IPC without a native folder
      // picker. Remount Settings through its real navigation to load the new
      // appointment before clicking the same restart control users receive.
      const other = [...document.querySelectorAll('aside nav button[aria-label]')]
        .find(element => !labels.settings.includes(element.getAttribute('aria-label')));
      assert(other, 'another page is available for Settings refresh');
      other.click();
      await wait(() => !labelledButton(labels.clear), 'Settings unmounted');
      labelledButton(labels.settings).click();
      restart = await wait(() => labelledButton(labels.restart), 'migration Restart and migrate action');
      panel = labelledButton(labels.clear).closest('.qp-subpanel');
      restart.scrollIntoView({ block: 'center' });
      assert(!restart.disabled && restart.getClientRects().length > 0, 'migration restart action visible and enabled');
    }
    await delay(1000); // Let WebKit persist localStorage before real app.restart.
    const visibleAlerts = [...document.querySelectorAll('[role="alert"]')]
      .filter(element => element.getClientRects().length && element.textContent.trim())
      .map(element => element.textContent.trim().slice(0, 300));
    const storageErrors = [...panel.querySelectorAll('[class*="text-[var(--qp-danger)]"]')]
      .filter(element => element.textContent.trim()).map(element => element.textContent.trim().slice(0, 300));
    assert(errors.length === 0 && visibleAlerts.length === 0 && storageErrors.length === 0,
      `page remains free of errors: ${JSON.stringify({ errors, visibleAlerts, storageErrors })}`);
    await report({ stage, passed: true, checks, before: compact(before), snapshot: compact(after), errors });
    if (stage !== 'verify-cache') {
      await wait(async () => {
        const response = await fetch(`/__storage_restart_ready?stage=${encodeURIComponent(stage)}`, {
          signal: AbortSignal.timeout(5000),
        });
        if (!response.ok) throw new Error(`Restart gate rejected: ${response.status}`);
        return (await response.json()).ready;
      }, 'native checks and next-stage continuation ready');
      assert(restart?.isConnected && !restart.disabled, 'real restart action ready to click');
      // This invokes SettingsStoragePanel -> StorageSettingsService -> the
      // production cmd_restart_for_storage_maintenance -> Tauri app.restart.
      restart.click();
    }
  } catch (error) {
    await report({ stage, passed: false, error: String(error), checks, before: compact(before), snapshot: compact(after), errors });
  }
})();
