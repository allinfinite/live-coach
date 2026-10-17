async (page) => {
  const results = [];
  const assert = (condition, label) => { if (!condition) throw new Error(label); results.push(label); };
  await page.setViewportSize({width:460,height:900});
  await page.addInitScript(() => {
    window.calls = [];
    window.pendingWarmups = [];
    window.remoteRequests = [];
    window.testIntervals = [];
    const originalSetInterval = window.setInterval.bind(window);
    window.setInterval = (callback, ms, ...args) => {
      window.testIntervals.push({callback,ms});
      return originalSetInterval(callback,ms,...args);
    };
    window.fetch = (...args) => { window.remoteRequests.push(String(args[0])); return Promise.reject(new Error('Unexpected frontend network request')); };
    window.__TAURI__ = {
      core: {invoke: async (command, args) => {
        window.calls.push({command,args});
        if (command === 'environment') return {
          models:[{path:'/read-only/parakeet',label:'Parakeet',engine:'parakeet'}],
          ollama:{available:localStorage.getItem('warmup-test-unavailable') !== 'true',models:['default-model:latest','saved-model:latest','other-model:latest']}
        };
        if (command === 'warm_ollama') return new Promise((resolve,reject) => window.pendingWarmups.push({resolve,reject,model:args.model}));
        return null;
      }},
      event: {listen: async (_name, handler) => { window.emitCoach = data => handler({payload:data}); return () => {}; }}
    };
  });
  async function launch(preferences) {
    await page.evaluate(prefs => localStorage.setItem('live-coach-preferences-v1',JSON.stringify(prefs)), preferences);
    await page.reload();
    await page.waitForFunction(() => document.querySelector('#ollamaModel').options.length === 3);
  }
  const calls = command => page.evaluate(cmd => window.calls.filter(c => c.command === cmd),command);
  await page.evaluate(() => localStorage.removeItem('warmup-test-unavailable'));
  await launch({provider:'ollama',model:'saved-model:latest'});
  await page.waitForFunction(() => window.pendingWarmups.length === 1);
  assert((await calls('warm_ollama'))[0].args.model === 'saved-model:latest', 'Startup warms the saved installed model');
  assert((await calls('warm_ollama'))[0].args.provider === 'ollama', 'Startup explicitly requests the local Ollama provider');
  assert((await calls('start_session')).length === 0, 'Startup warm-up never starts audio capture');
  assert(!(await page.locator('#consent').isChecked()), 'Warm-up leaves consent unchecked');
  assert(await page.locator('#consent').isEnabled(), 'Consent stays available during pending warm-up');
  assert(await page.locator('#start').isEnabled(), 'Start remains available while warm-up is pending');
  assert(await page.locator('#start').textContent() === 'Start conversation', 'Warm-up leaves conversation stopped');
  assert((await calls('warm_ollama')).length === 1, 'Pending startup does not duplicate warm-up');
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('Warming local coach')), 'Pending warm-up has visible status');
  assert(await page.evaluate(() => calls.every(c => ['environment','warm_ollama','cancel_warmup','voice_status'].includes(c.command))), 'Startup requests only environment and warm-up, never downloads or coaching');
  await page.locator('#start').click();
  assert(await page.locator('#notice').textContent().then(text => text.includes('Acknowledge')), 'Consent still blocks Start during warm-up');
  assert((await calls('start_session')).length === 0, 'Rejected consent never reaches audio backend');
  await page.evaluate(() => window.pendingWarmups[0].resolve());
  await page.waitForFunction(() => document.querySelector('#warmupStatus').textContent.includes('Local coach ready'));
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('saved-model:latest')), 'Ready status identifies warmed model');
  assert(await page.locator('#warmupRetry').isHidden(), 'Successful warm-up hides retry');
  await page.locator('#settingsToggle').click();
  await page.locator('#ollamaModel').selectOption('other-model:latest');
  await page.waitForFunction(() => window.pendingWarmups.length === 2);
  assert(await page.evaluate(() => JSON.parse(localStorage.getItem('live-coach-preferences-v1')).model === 'other-model:latest'), 'Changing local model persists preference');
  await page.evaluate(() => window.pendingWarmups[1].reject(new Error('Ollama temporarily unavailable')));
  await page.waitForFunction(() => !document.querySelector('#warmupRetry').hidden);
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('Ollama temporarily unavailable')), 'Warm-up failure displays actionable error');
  assert(await page.locator('#start').isEnabled(), 'Warm-up failure does not disable Start');
  await page.locator('#warmupRetry').click();
  await page.waitForFunction(() => window.pendingWarmups.length === 3);
  assert((await calls('warm_ollama'))[2].args.model === 'other-model:latest', 'Retry warms current selected model');
  await page.locator('#ollamaModel').selectOption('default-model:latest');
  await page.waitForFunction(() => window.pendingWarmups.length === 4);
  await page.evaluate(() => window.pendingWarmups[2].resolve());
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('Warming local coach') && text.includes('default-model:latest')), 'Stale model success cannot overwrite current pending status');
  await page.locator('#provider').selectOption('openai');
  await page.waitForFunction(() => window.calls.some(c => c.command === 'cancel_warmup'));
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('local warm-up off')), 'Selecting external coaching disables local warm-up');
  await page.evaluate(() => window.pendingWarmups[3].reject(new Error('stale cancellation')));
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('local warm-up off')), 'Stale local failure cannot overwrite external provider status');
  assert(await page.locator('#warmupRetry').isHidden(), 'Stale local failure does not expose retry for external provider');
  assert(await page.evaluate(() => remoteRequests.length === 0), 'Selecting external provider sends no transcript or frontend request');
  const countBeforeFocus = (await calls('warm_ollama')).length;
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  assert((await calls('warm_ollama')).length === countBeforeFocus, 'Focusing with external provider does not warm Ollama');
  await launch({provider:'openai',model:'saved-model:latest',cloud_model:'external-example',endpoint:'https://example.invalid/v1'});
  assert((await calls('warm_ollama')).length === 0, 'Saved external provider skips startup warm-up');
  assert((await calls('start_session')).length === 0, 'Saved external provider startup never captures audio');
  assert(await page.evaluate(() => remoteRequests.length === 0), 'Saved external provider startup makes no frontend network request');
  assert(!(await page.locator('#consent').isChecked()), 'External provider startup also leaves consent unchecked');
  await page.locator('#settingsToggle').click();
  await page.locator('#provider').selectOption('ollama');
  await page.waitForFunction(() => window.pendingWarmups.length === 1);
  assert((await calls('warm_ollama'))[0].args.model === 'saved-model:latest', 'Switching back to local warms saved local model');
  await page.evaluate(() => window.pendingWarmups[0].resolve());
  await page.waitForFunction(() => document.querySelector('#warmupStatus').textContent.includes('Local coach ready'));
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await page.waitForFunction(() => window.pendingWarmups.length === 2);
  assert((await calls('warm_ollama')).length === 2, 'App focus refreshes local readiness after previous warm-up completes');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  assert((await calls('warm_ollama')).length === 2, 'Repeated focus deduplicates an in-flight warm-up');
  assert((await calls('start_session')).length === 0, 'All readiness checks leave audio stopped');
  await page.evaluate(() => window.pendingWarmups[1].resolve());
  await page.waitForFunction(() => document.querySelector('#warmupStatus').textContent.includes('Local coach ready'));
  await page.evaluate(() => window.testIntervals.find(interval => interval.ms === 600000).callback());
  await page.waitForFunction(() => window.pendingWarmups.length === 3);
  assert((await calls('warm_ollama')).length === 3, 'Ten-minute readiness timer rechecks local model');
  await page.locator('#simulation').check();
  await page.locator('#consent').check();
  await page.locator('#start').click();
  assert((await calls('start_session')).length === 1, 'Explicit consented Start proceeds while warm-up remains pending');
  assert(await page.locator('#start').textContent() === 'Stop conversation', 'Pending warm-up does not block conversation state');
  await page.locator('#start').click();
  await page.evaluate(() => window.pendingWarmups[2].resolve());
  await page.evaluate(() => localStorage.setItem('warmup-test-unavailable','true'));
  await launch({provider:'ollama',model:'saved-model:latest'});
  await page.waitForFunction(() => !document.querySelector('#warmupRetry').hidden);
  assert((await calls('warm_ollama')).length === 0, 'Unavailable Ollama is never warmed or downloaded');
  assert(await page.locator('#warmupStatus').textContent().then(text => text.includes('open Ollama')), 'Unavailable Ollama gives setup guidance');
  assert(await page.locator('#start').isEnabled(), 'Unavailable Ollama leaves Start available');
  assert((await calls('start_session')).length === 0 && !(await page.locator('#consent').isChecked()), 'Unavailable Ollama leaves capture stopped and consent unchecked');
  await page.evaluate(() => localStorage.removeItem('warmup-test-unavailable'));
  await page.locator('#warmupRetry').click();
  await page.waitForFunction(() => window.pendingWarmups.length === 1);
  assert((await calls('warm_ollama'))[0].args.model === 'saved-model:latest', 'Retry rediscovers Ollama after it becomes available');
  await page.evaluate(() => window.pendingWarmups[0].resolve());
  await page.waitForFunction(() => document.querySelector('#warmupStatus').textContent.includes('Local coach ready'));
  assert((await calls('start_session')).length === 0, 'Recovery retry never starts audio');
  return {passed:results.length,results};
}
