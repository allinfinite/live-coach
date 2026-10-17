async (page) => {
  const results = [];
  const assert = (value, label) => { if (!value) throw new Error(label); results.push(label); };
  await page.addInitScript(() => {
    localStorage.clear(); window.calls = []; window.handlers = {}; window.folderCancelled = false;
    window.__TAURI__ = {core:{invoke:async(command,args) => {
      calls.push({command,args});
      if (command === 'environment') return {models:[],ollama:{available:true,models:['local']}};
      if (command === 'voice_status') return {};
      if (command === 'codex_connect') return {connected:true,models:[{id:'codex-model'}]};
      if (command === 'codex_choose_project' || command === 'codex_project') return folderCancelled ? null : {project:args?.path || '/chosen/project',files:[{path:'README.md',bytes:10}],truncated:false};
      if (command === 'codex_history_list') {
        const response = {threads:[{id:args.cursor?'thread-2':'thread-1',title:args.cursor?'Older conversation':'<img src=x onerror="window.injected=true"> Selected project',project:'/history/project',updated_at:'2026-10-16'}],next_cursor:args.cursor?null:'next-page'};
        if (window.delayList) return new Promise(resolve=>window.finishList=()=>resolve(response));
        return response;
      }
      if (command === 'codex_history_read') {
        const response = {snapshot_id:'history-token',thread_id:args.threadId,title:'Conversation title',messages:[{id:'user-1',role:'user',text:'Review the indigo release plan.'},{id:'assistant-1',role:'assistant',text:'<script>window.injected=true</script> Tuesday is the launch day.'}],truncated:true};
        if (window.delayRead) return new Promise(resolve=>window.finishRead=()=>resolve(response));
        return response;
      }
      if (command === 'codex_prepare') return {review_id:'review-token',project:args.project,model:args.model,mode:args.mode,provider:'OpenAI',scope:'Read selected project',task:args.task,files:[],history:args.historyItems.map(id=>({title:'Conversation title',thread_id:'thread-1',role:id==='user-1'?'user':'assistant',text:id==='user-1'?'Review the indigo release plan.':'Tuesday is the launch day.'})),prompt:'Exact reviewed prompt'};
      if (command === 'codex_send') return {run_id:'run-1'};
      return null;
    }},event:{listen:async(name,handler)=>{handlers[name]=handler;return ()=>delete handlers[name];}}};
    window.emitCodex = data => handlers['codex-event']?.({payload:data});
  });
  await page.reload(); await page.waitForFunction(()=>calls.some(c=>c.command==='environment'));
  const count = command => page.evaluate(cmd=>calls.filter(c=>c.command===cmd).length,command);
  await page.locator('#codexPanel').evaluate(e=>e.open=true);
  assert(await page.evaluate(()=>calls.every(c=>!c.command.startsWith('codex_'))),'Opening the app and panel never lists private history');
  await page.locator('#codexConnect').click();
  assert(await count('codex_history_list')===0 && await count('codex_history_read')===0,'Connecting does not browse or read history');
  assert(await page.locator('#codexProjectPath').isHidden(),'Manual path entry is optional and collapsed');
  await page.locator('#codexChooseProject').click();
  assert(await count('codex_choose_project')===1 && (await page.locator('#codexProjectStatus').textContent()).includes('/chosen/project'),'Native folder selection loads the chosen project without typing');
  assert(await page.locator('#codexFiles input:checked').count()===0,'Folder chooser never attaches files automatically');
  await page.locator('#codexBrowseHistory').click();
  assert(await count('codex_history_list')===1 && await count('codex_history_read')===0,'Explicit history browsing reads metadata before message text');
  assert(await page.locator('#codexHistoryThreads img').count()===0 && !(await page.evaluate(()=>window.injected)),'Conversation titles render as safe text');
  await page.locator('#codexHistoryMore').click();
  assert(await page.locator('#codexHistoryThreads section').count()===2,'More conversations appends the next page');
  assert(await page.evaluate(()=>calls.filter(c=>c.command==='codex_history_list').at(-1).args.cursor==='next-page'),'History pagination passes the backend cursor');
  assert(await page.locator('#codexHistoryMore').isHidden(),'The last history page hides More');
  await page.locator('#codexHistoryThreads button').filter({hasText:'Preview messages'}).first().click();
  assert(await count('codex_history_read')===1 && await page.locator('#codexHistoryMessages input:checked').count()===0,'History preview leaves every message unselected');
  assert(await page.locator('#codexHistoryMessages script').count()===0 && !(await page.evaluate(()=>window.injected)),'User and assistant message text never executes HTML');
  assert((await page.locator('#codexHistoryStatus').textContent()).includes('limited'),'Truncated history previews disclose their limit');
  await page.locator('#codexPrompt').fill('Make a launch checklist.'); await page.locator('#codexPrepare').click();
  assert(await page.evaluate(()=>{const a=calls.filter(c=>c.command==='codex_prepare').at(-1).args;return a.historySnapshot===null&&a.historyItems.length===0;}),'Previewing history attaches no messages to a review');
  await page.locator('#codexHistoryMessages input').first().check();
  assert(await page.locator('#codexReview').isHidden(),'Selecting history invalidates an existing review');
  await page.locator('#codexPrepare').click();
  assert(await page.evaluate(()=>{const a=calls.filter(c=>c.command==='codex_prepare').at(-1).args;return a.historySnapshot==='history-token'&&JSON.stringify(a.historyItems)==='["user-1"]';}),'Prepare includes only individually selected history IDs and its snapshot');
  const reviewed=await page.locator('#codexReviewText').textContent();
  assert(reviewed.includes('Conversation title')&&reviewed.includes('user')&&reviewed.includes('indigo')&&!reviewed.includes('Tuesday'),'Review shows provenance and selected history text only');
  assert(await count('codex_send')===0,'Browsing, selecting and reviewing history never starts a task');
  await page.locator('#codexHistoryClear').click();
  assert(await page.locator('#codexReview').isHidden() && await page.locator('#codexHistoryPreview').isHidden(),'Clear history removes selected context and invalidates review');
  await page.locator('#codexHistorySearch').fill('launch');
  assert(await count('codex_history_list')===2,'Typing search does not automatically browse history');
  await page.locator('#codexHistorySearchButton').click();
  assert(await page.evaluate(()=>{const a=calls.filter(c=>c.command==='codex_history_list').at(-1).args;return a.search==='launch'&&a.cursor===null;}),'Explicit search resets the cursor and sends the query');
  await page.locator('#codexHistoryProjectOnly').check();
  assert(await page.evaluate(()=>calls.filter(c=>c.command==='codex_history_list').at(-1).args.project==='/chosen/project'),'Current project filter uses the selected project');
  await page.locator('#codexHistoryThreads button').filter({hasText:'Use this project'}).first().click();
  assert(await page.evaluate(()=>calls.filter(c=>c.command==='codex_project').at(-1).args.path==='/history/project'),'A history project can be chosen without entering its path');
  assert(await page.locator('#codexHistoryPreview').isHidden() && await page.locator('#codexHistoryMessages input').count()===0,'Changing project clears prior history selections');
  await page.evaluate(()=>window.folderCancelled=true); await page.locator('#codexChooseProject').click();
  assert(await page.locator('#codexPrepare').isDisabled() && (await page.locator('#codexStatus').textContent()).includes('cancelled'),'Cancelling folder selection leaves no old project reviewable');
  await page.evaluate(()=>window.folderCancelled=false); await page.locator('#codexChooseProject').click();
  await page.locator('#codexBrowseHistory').click(); await page.evaluate(()=>window.delayRead=true);
  await page.locator('#codexHistoryThreads button').filter({hasText:'Preview messages'}).first().click();
  await page.waitForFunction(()=>typeof finishRead==='function');
  await page.locator('#codexDisconnect').click(); await page.evaluate(()=>finishRead());
  assert(await page.locator('#codexSetup').isHidden() && await page.locator('#codexHistoryMessages input').count()===0,'History arriving after disconnect is discarded');
  await page.locator('#codexConnect').click(); await page.evaluate(()=>window.delayList=true);
  await page.locator('#codexBrowseHistory').click(); await page.waitForFunction(()=>typeof finishList==='function');
  await page.locator('#codexHistorySearch').evaluate(e=>{e.value='changed';e.dispatchEvent(new Event('input',{bubbles:true}));});
  await page.evaluate(()=>finishList());
  assert(await page.locator('#codexHistoryThreads section').count()===0,'A history list arriving after its search changed is discarded');
  await page.evaluate(()=>window.delayList=false); await page.locator('#codexHistorySearchButton').click();
  await page.locator('#codexDisconnect').click();
  assert(await page.locator('#codexHistoryThreads section').count()===0 && await page.locator('#codexHistory').isHidden(),'Disconnect clears private history previews and metadata');
  assert(await page.evaluate(()=>calls.every(c=>!['start_session','start_voice_reference','speak_suggestion','thread_resume','codex_send'].includes(c.command))),'Pickers never start audio, resume old tasks or send new tasks');
  return {passed:results.length,results};
}
