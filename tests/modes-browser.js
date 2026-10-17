async (page) => {
  const results=[];
  const assert=(value,label)=>{if(!value)throw new Error(label);results.push(label)};
  await page.addInitScript(()=>{
    localStorage.setItem('live-coach-preferences-v1',JSON.stringify({profile:'legacy',profiles:{default:{name:'General conversation',context:''},legacy:{name:'A difficult conversation',context:'I need to leave at six.'}}}));
    window.calls=[];
    window.__TAURI__={core:{invoke:async(command,args)=>{
      calls.push({command,args});
      if(command==='environment')return {models:[],ollama:{available:true,models:['local-model']}};
      if(command==='stop_session')window.emitCoach({type:'stopped'});
      return null;
    }},event:{listen:async(_,handler)=>{window.emitCoach=payload=>handler({payload});}}};
  });
  await page.reload();
  await page.waitForFunction(()=>document.querySelector('#ollamaModel').value==='local-model');
  await page.locator('#settingsToggle').click();
  assert(await page.locator('#mode').inputValue()==='personal','Legacy profiles default to Personal coach');
  assert(await page.locator('#context').inputValue()==='I need to leave at six.','Legacy relationship context is preserved');
  assert(await page.locator('#goal').inputValue()==='' && await page.locator('#intention').inputValue()==='','Legacy profiles do not invent a goal or intention');
  await page.locator('#goal').fill('Keep my boundary clear');await page.locator('#goal').dispatchEvent('change');
  await page.locator('#intention').fill('Be direct and kind');await page.locator('#intention').dispatchEvent('change');
  await page.locator('#simulation').check();
  for(const mode of ['personal','sales','secretary']){
    await page.locator('#mode').selectOption(mode);
    assert(await page.locator('#goal').inputValue()==='Keep my boundary clear',`${mode} mode preserves the chosen goal`);
    await page.locator('#consent').check();await page.locator('#start').click();
    const settings=await page.evaluate(()=>calls.filter(c=>c.command==='start_session').at(-1).args.settings);
    assert(settings.mode===mode && settings.goal==='Keep my boundary clear' && settings.intention==='Be direct and kind' && settings.context==='I need to leave at six.',`${mode} mode sends purpose and context to the backend`);
    assert(await page.locator('#mode').isDisabled() && await page.locator('#goal').isDisabled(),`${mode} purpose is stable during the conversation`);
    await page.locator('#start').click();
  }
  const stored=await page.evaluate(()=>JSON.parse(localStorage.getItem('live-coach-preferences-v1')).profiles.legacy);
  assert(stored.mode==='secretary' && stored.goal==='Keep my boundary clear' && stored.intention==='Be direct and kind','Existing profile saves mode, goal and intention');
  assert(stored.name==='A difficult conversation' && stored.context==='I need to leave at six.','Saving preserves legacy profile name and context');
  await page.locator('#profile').selectOption('default');
  assert(await page.locator('#mode').inputValue()==='personal' && await page.locator('#goal').inputValue()==='','Switching profile restores its own purpose');
  await page.locator('#profile').selectOption('legacy');
  assert(await page.locator('#mode').inputValue()==='secretary' && await page.locator('#intention').inputValue()==='Be direct and kind','Switching back restores the saved purpose');
  assert(await page.evaluate(()=>!calls.some(c=>/speak|reference|command|agent/.test(c.command))),'Changing coaching presets never starts voice or an agent action');
  return {passed:results.length,results};
}
