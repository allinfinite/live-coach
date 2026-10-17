'use strict';
const $ = id => document.getElementById(id);
const invoke = (command, args = {}) => {
  if (!window.__TAURI__?.core?.invoke) return Promise.reject(new Error('Open Live Coach as a macOS application to use audio and coaching.'));
  return window.__TAURI__.core.invoke(command, args);
};
let active = false, busy = false, timerInterval = null, deadline = 0, transcriptIds = new Set(), localModels = [], environment = {};
const storageKey = 'live-coach-preferences-v1';
let preferences = {};
try { preferences = JSON.parse(localStorage.getItem(storageKey) || '{}'); } catch (_) {}
const modeDescriptions = {
  personal: 'Find clear words, hold boundaries, and make room for repair.',
  sales: 'Understand needs, check fit honestly, and agree on a useful next step.',
  secretary: 'Clarify decisions, commitments, owners, and follow-up.'
};
const validMode = mode => Object.hasOwn(modeDescriptions, mode) ? mode : 'personal';
let profiles = preferences.profiles || {default:{name:'Personal conversation',context:''}};
function profileFields() { return {mode:validMode($('mode').value),goal:$('goal').value,intention:$('intention').value,context:$('context').value}; }
function modeUI() { $('modeNote').textContent = modeDescriptions[validMode($('mode').value)]; }
function loadProfile() {
  const profile=profiles[$('profile').value] || {};
  $('mode').value=validMode(profile.mode);
  $('goal').value=profile.goal || '';
  $('intention').value=profile.intention || '';
  $('context').value=profile.context || '';
  modeUI();
}
function notice(message) { $('notice').textContent = message; $('notice').hidden = !message; }
function errorMessage(error) { return typeof error === 'string' ? error : error?.message || String(error); }
function savePreferences() {
  preferences = {profiles,profile:$('profile').value,quiet:$('quiet').checked,timer_minutes:Number($('timerMinutes').value)||10,provider:$('provider').value,model:$('ollamaModel').value,cloud_model:$('cloudModel').value,endpoint:$('endpoint').value,retain:$('retain').checked,retention_days:Number($('retentionDays').value)||7,stt_path:$('sttModel').value};
  localStorage.setItem(storageKey, JSON.stringify(preferences));
}
let voice = {runtime_ready:false,has_voice:false,recording:false,playing:false,generating:false};
let voiceBusy = false, voicePolling = null, voiceStartedAt = 0, voiceEnrolling = false, hasSuggestion = false;
function voiceIsPlaying() { return !!(voice.playing || voice.generating); }
function renderVoice() {
  const enrolling = voiceEnrolling || voice.recording || voice.pending_sample;
  const speaking = voiceIsPlaying();
  $('recordVoice').disabled = active || voiceBusy || enrolling || speaking || !voice.runtime_ready || !$('voiceConsent').checked;
  $('voiceConsent').disabled = active || voiceBusy || enrolling || speaking;
  $('recordVoice').hidden = !!enrolling;
  $('saveVoice').hidden = !enrolling;
  $('cancelVoice').hidden = !enrolling;
  const elapsed = voiceStartedAt ? Math.min(30,Math.floor((Date.now()-voiceStartedAt)/1000)) : 0;
  $('saveVoice').disabled = voiceBusy || elapsed < 5;
  $('cancelVoice').disabled = voiceBusy;
  $('previewVoice').hidden = !voice.has_voice || !!enrolling;
  $('previewVoice').disabled = active || voiceBusy || speaking || !voice.runtime_ready;
  $('forgetVoice').hidden = !voice.has_voice || !!enrolling;
  $('forgetVoice').disabled = active || voiceBusy || speaking;
  $('voiceScript').textContent = voice.reference_text || 'Today I am taking a moment to listen, think, and find my own words. I can be clear and kind, ask for what I need, and give myself time to respond. A calm conversation leaves room for both people to be heard.';
  $('voiceScript').hidden = !!voice.has_voice && !enrolling;
  $('readSuggestion').disabled = !voice.runtime_ready || !voice.has_voice || !hasSuggestion || voiceBusy || !!enrolling || speaking;
  $('stopVoice').hidden = !speaking;
  $('stopVoice').disabled = false;
  $('start').disabled = busy || !!enrolling || (!active && speaking);
  if (enrolling) $('voiceStatus').textContent = elapsed >= 30 ? 'Sample complete · 30s · save when ready' : `Voice sample · ${elapsed}s / 30s · ${elapsed < 5 ? 'keep reading' : 'save when ready'}`;
  else if (!voice.runtime_ready) $('voiceStatus').textContent = 'Local voice setup required. See the installation guide. No model is downloaded automatically.';
  else $('voiceStatus').textContent = voice.has_voice ? 'Your voice is ready · local playback only' : 'Ready to record your voice · conversation must be stopped';
  $('voicePlaybackStatus').textContent = voice.generating ? 'Preparing your voice…' : voice.playing ? 'Reading aloud · transcription paused' : '';
  if ((enrolling || speaking) && !voicePolling) voicePolling = setInterval(()=>{renderVoice();void refreshVoice();},1000);
  if (!enrolling && !speaking && voicePolling) {clearInterval(voicePolling);voicePolling=null;}
}
async function refreshVoice() {
  try {const state=await invoke('voice_status');if(state){voice={...voice,...state};renderVoice();}}
  catch (_) {$('voiceStatus').textContent='Unable to check local voice. Reopen Live Coach to retry.';}
}
$('voiceConsent').onchange=renderVoice;
$('recordVoice').onclick=async()=>{
  if(active || voiceBusy || !$('voiceConsent').checked) return;
  voiceBusy=true;renderVoice();notice('');
  try {await invoke('start_voice_reference',{consent:true});voiceEnrolling=true;voiceStartedAt=Date.now();voice.recording=true;}
  catch(error){notice(errorMessage(error));}
  finally{voiceBusy=false;renderVoice();await refreshVoice();}
};
$('saveVoice').onclick=async()=>{
  if(voiceBusy || Date.now()-voiceStartedAt<5000)return;
  voiceBusy=true;renderVoice();
  try {await invoke('finish_voice_reference');voiceEnrolling=false;voiceStartedAt=0;voice.recording=false;voice.pending_sample=false;voice.has_voice=true;$('voiceConsent').checked=false;notice('Your voice sample is saved on this Mac.');}
  catch(error){voiceEnrolling=false;voiceStartedAt=0;voice.recording=false;voice.pending_sample=false;notice(errorMessage(error));}
  finally{voiceBusy=false;await refreshVoice();renderVoice();}
};
$('cancelVoice').onclick=async()=>{
  if(voiceBusy)return;voiceBusy=true;renderVoice();
  try{await invoke('cancel_voice_reference');voiceEnrolling=false;voiceStartedAt=0;voice.recording=false;voice.pending_sample=false;$('voiceConsent').checked=false;}
  catch(error){notice(errorMessage(error));}
  finally{voiceBusy=false;await refreshVoice();renderVoice();}
};
$('forgetVoice').onclick=async()=>{
  if(active || voiceBusy)return;voiceBusy=true;renderVoice();
  try{await invoke('forget_voice');voice.has_voice=false;$('voiceConsent').checked=false;notice('Your saved voice sample has been deleted.');}
  catch(error){notice(errorMessage(error));}
  finally{voiceBusy=false;await refreshVoice();renderVoice();}
};
$('readSuggestion').onclick=async()=>{
  if(!hasSuggestion || !voice.has_voice || voiceBusy || voiceIsPlaying())return;
  voice.generating=true;renderVoice();notice('');
  try{await invoke('speak_suggestion');}
  catch(error){notice(errorMessage(error));}
  finally{voice.generating=false;voice.playing=false;await refreshVoice();renderVoice();}
};
$('previewVoice').onclick=async()=>{
  if(active || !voice.has_voice || voiceBusy || voiceIsPlaying())return;
  voice.generating=true;renderVoice();notice('');
  try{await invoke('preview_voice');}
  catch(error){notice(errorMessage(error));}
  finally{voice.generating=false;voice.playing=false;await refreshVoice();renderVoice();}
};
$('stopVoice').onclick=async()=>{
  try{await invoke('stop_voice');voice.generating=false;voice.playing=false;await refreshVoice();renderVoice();}
  catch(error){notice(errorMessage(error));}
};
let warmupGeneration = 0;
let warmingModel = null;
async function warmLocalCoach() {
  const provider = $('provider').value;
  const model = $('ollamaModel').value;
  if (provider === 'ollama' && warmingModel === model) return;
  const generation = ++warmupGeneration;
  warmingModel = null;
  $('warmupRetry').hidden = true;
  if (provider !== 'ollama') {
    $('warmupStatus').textContent = 'External coaching selected · local warm-up off';
    try { await invoke('cancel_warmup'); } catch (_) {}
    return;
  }
  if (!model || !environment.ollama?.available) {
    $('warmupStatus').textContent = 'Local coach unavailable · open Ollama and refresh models';
    $('warmupRetry').hidden = false;
    return;
  }
  warmingModel = model;
  $('warmupStatus').textContent = `Warming local coach · ${model}…`;
  try {
    await invoke('warm_ollama', {provider:'ollama', model});
    if (generation === warmupGeneration) $('warmupStatus').textContent = `Local coach ready · ${model}`;
  } catch (error) {
    if (generation === warmupGeneration) {
      $('warmupStatus').textContent = `Local coach not ready · ${errorMessage(error)}`;
      $('warmupRetry').hidden = false;
    }
  } finally {
    if (generation === warmupGeneration) warmingModel = null;
  }
}
function renderProfiles() {
  const current = $('profile').value;
  $('profile').replaceChildren(...Object.entries(profiles).map(([id,p])=>new Option(p.name,id)));
  $('profile').value = profiles[current] ? current : 'default';
}
renderProfiles();
if(profiles[preferences.profile]) $('profile').value = preferences.profile;
loadProfile();
$('quiet').checked = !!preferences.quiet;
$('timerMinutes').value = preferences.timer_minutes || 10;
$('provider').value = preferences.provider || 'ollama';
$('cloudModel').value = preferences.cloud_model || '';
$('endpoint').value = preferences.endpoint || 'https://api.venice.ai/api/v1';
$('retain').checked = !!preferences.retain;
$('retentionDays').value = preferences.retention_days || 7;
function captureNote(){ $('captureNote').textContent=$('simulation').checked?'Practice transcript · no audio captured':($('provider').value==='openai'?'Local audio capture · external coaching · no audio files saved':'Local listening & coaching · microphone + system audio · no audio files saved'); }
function providerUI() { const cloud = $('provider').value === 'openai'; $('cloudSettings').hidden=!cloud; $('localSettings').hidden=cloud; $('cloudBanner').hidden=!cloud; captureNote(); }
function retentionUI() { $('retentionLabel').hidden=!$('retain').checked; $('retentionStatus').textContent=$('retain').checked?`Retained locally for ${$('retentionDays').value} days.`:'Transcript disappears when you stop.'; }
function timerText() { const seconds = active ? Math.max(0,Math.ceil((deadline-Date.now())/1000)) : Math.max(1,Number($('timerMinutes').value)||10)*60; $('timer').textContent=`${Math.floor(seconds/60).toString().padStart(2,'0')}:${(seconds%60).toString().padStart(2,'0')}`; if(active && seconds===0){clearInterval(timerInterval);timerInterval=null;notice('Your check-in timer is complete. Take a breath and decide whether to continue.');} }
function setActive(value) {
  active=value;if(!value)hasSuggestion=false; document.body.classList.toggle('active',value);$('start').textContent=value?'Stop conversation':'Start conversation';$('consent').disabled=value;$('consent').checked=false;
  document.querySelectorAll('[data-action]').forEach(button=>button.disabled=!value);
  for(const input of $('settings').querySelectorAll('input,select,textarea,button')) input.disabled=value;
  $('quiet').disabled=value;
  if(value){deadline=Date.now()+Math.max(1,Number($('timerMinutes').value)||10)*60000;timerInterval=setInterval(timerText,1000)}else{clearInterval(timerInterval);timerInterval=null;$('cloudConsent').checked=false;$('status').lastChild.textContent=' Ready when you are';}
  timerText();renderVoice();
}
function settings() {
  const selected=localModels.find(m=>m.path===$('sttModel').value);
  return {model:$('provider').value==='ollama'?$('ollamaModel').value:$('cloudModel').value.trim(),provider:$('provider').value==='ollama'?'ollama':'openai_compatible',endpoint:$('endpoint').value.trim(),cloud_url:$('endpoint').value.trim(),cloud_consent:$('cloudConsent').checked,quiet:$('quiet').checked,quiet_mode:$('quiet').checked,context:$('context').value.trim(),mode:validMode($('mode').value),goal:$('goal').value.trim(),intention:$('intention').value.trim(),retention_days:Math.max(1,Math.min(365,Number($('retentionDays').value)||7)),retain:$('retain').checked,timer_minutes:Math.max(1,Math.min(120,Number($('timerMinutes').value)||10)),stt_path:$('sttModel').value,stt_engine:selected?.engine||'whisper'};
}
function clearTranscript(){hasSuggestion=false;renderVoice();transcriptIds.clear();$('transcript').replaceChildren();$('segmentCount').textContent='Live transcript';$('insight').textContent='Take a moment to breathe. Start a conversation when you’re ready.';$('reply').textContent='“I’d like a moment to think about that.”';$('confidence').textContent='Waiting';}
function addTranscript(segment){if(!segment.text?.trim()||transcriptIds.has(segment.id))return;transcriptIds.add(segment.id);const box=$('transcript'),nearBottom=box.scrollHeight-box.scrollTop-box.clientHeight<65;box.querySelector('.empty')?.remove();const row=document.createElement('div');row.className='segment';if(segment.speaker){const speaker=document.createElement('span');speaker.className='speaker';speaker.textContent=segment.speaker;row.append(speaker)}const p=document.createElement('p');p.textContent=segment.text;row.append(p);box.append(row);while(box.children.length>100)box.firstElementChild.remove();$('segmentCount').textContent=`${transcriptIds.size} segments`;if(nearBottom)box.scrollTop=box.scrollHeight;}
function coachEvent(event){const data=event.payload||event;switch(data.type){case'status':$('status').lastChild.textContent=` ${data.message}`;break;case'transcript':addTranscript(data);break;case'insight':hasSuggestion=!!data.suggested_response?.trim();renderVoice();$('insight').textContent=data.observation;$('reply').textContent=data.suggested_response;$('confidence').textContent=data.confidence||'Consider';break;case'error':notice(data.message);break;case'stopped':setActive(false);if(!$('retain').checked)clearTranscript();break;}}
async function refresh(){ $('refresh').disabled=true;try{environment=await invoke('environment');localModels=environment.models||[];$('sttModel').replaceChildren(...(localModels.length?localModels.map(m=>new Option(m.label||m.path,m.path)):[new Option('No compatible speech model found','')]));if(localModels.some(m=>m.path===preferences.stt_path))$('sttModel').value=preferences.stt_path;$('modelNote').textContent=localModels.length?'Existing models are reused read-only. No model is downloaded automatically.':'Install a compatible local speech model before listening. The simulator is available without one. No model will be downloaded automatically.';const ollama=environment.ollama||{};const models=ollama.models||[];$('ollamaModel').replaceChildren(...(models.length?models.map(m=>new Option(typeof m==='string'?m:m.name,typeof m==='string'?m:m.name)):[new Option('No Ollama models available','')]));if(models.includes(preferences.model))$('ollamaModel').value=preferences.model;$('ollamaNote').textContent=ollama.available?(models.length?'Ollama is available on this Mac.':'Ollama is running. Install a model in Ollama before requesting coaching.'):'Ollama is not running. Open Ollama to enable local coaching.';$('environmentNote').textContent=environment.message||'';void warmLocalCoach();}catch(error){notice(errorMessage(error));$('modelNote').textContent='Unable to inspect speech models.';}finally{$('refresh').disabled=active;}}
$('start').onclick=async()=>{if(busy||voiceEnrolling||voice.recording||voice.pending_sample||(!active&&voiceIsPlaying()))return;busy=true;$('start').disabled=true;const wasActive=active;if(!active){$('start').textContent='Starting…';$('status').lastChild.textContent=' Preparing audio and speech model…';$('consent').disabled=true;for(const input of $('settings').querySelectorAll('input,select,textarea,button'))input.disabled=true;$('quiet').disabled=true;}try{if(active){await invoke('stop_session');setActive(false);if(!$('retain').checked)clearTranscript();notice('Conversation stopped. Audio capture has ended.');}else{if(!$('consent').checked)throw new Error('Acknowledge that participants have been informed before starting.');if($('provider').value==='openai'&&!$('cloudConsent').checked)throw new Error('Acknowledge external transcript processing in Conversation settings first.');if(!$('simulation').checked&&!$('sttModel').value)throw new Error('No compatible speech model is available. Choose the transcript simulator or install a compatible model.');savePreferences();clearTranscript();notice('');$('confidence').textContent='Listening';await invoke('start_session',{consent:true,simulation:$('simulation').checked,settings:settings()});setActive(true);$('status').lastChild.textContent=$('simulation').checked?' Simulator running':' Listening · microphone + system audio';}}catch(error){notice(errorMessage(error));}finally{busy=false;$('start').disabled=false;if(!active){$('start').textContent='Start conversation';$('consent').disabled=false;$('quiet').disabled=false;for(const input of $('settings').querySelectorAll('input,select,textarea,button'))input.disabled=false;if(!wasActive)$('status').lastChild.textContent=' Ready when you are';}renderVoice();}};
document.querySelectorAll('[data-action]').forEach(button=>button.onclick=async()=>{button.disabled=true;notice('');try{await invoke('quick_action',{action:button.dataset.action});}catch(error){notice(errorMessage(error));}finally{button.disabled=!active;}});
$('delete').onclick=async()=>{try{await invoke('delete_transcripts');clearTranscript();$('insight').textContent='Conversation text has been deleted.';$('reply').textContent='“Let me take a moment.”';$('confidence').textContent='Cleared';$('history').replaceChildren();$('historyPanel').hidden=true;setActive(false);$('retain').checked=false;retentionUI();savePreferences();notice('Conversation stopped. Live context and retained transcripts deleted.');}catch(error){notice(errorMessage(error));}};
$('viewHistory').onclick=async()=>{try{const sessions=await invoke('load_history');const history=$('history');history.replaceChildren();for(const session of sessions||[]){const heading=document.createElement('p');heading.className='subtle';heading.textContent=`Expires ${new Date(session.expires_at*1000).toLocaleString()}`;history.append(heading);for(const segment of session.segments||[]){const text=document.createElement('p');text.className='history-segment';text.textContent=(segment.speaker?segment.speaker+': ':'')+segment.text;history.append(text);}}if(!history.children.length){const empty=document.createElement('p');empty.className='subtle';empty.textContent='No retained transcripts.';history.append(empty);}$('historyPanel').hidden=false;$('historyPanel').open=true;$('historyPanel').scrollIntoView({behavior:'smooth',block:'start'});}catch(error){notice(errorMessage(error));}};
$('saveKey').onclick=async()=>{const key=$('apiKey').value.trim();if(!key){notice('Enter an API key to save.');return;}try{await invoke('save_api_key',{key});$('apiKey').value='';notice('API key saved in macOS Keychain.');}catch(error){notice(errorMessage(error));}};
$('forgetKey').onclick=async()=>{try{await invoke('save_api_key',{key:''});$('apiKey').value='';notice('API key removed from macOS Keychain.');}catch(error){notice(errorMessage(error));}};
$('saveProfile').onclick=()=>{const name=prompt('Name this conversation profile');if(!name?.trim())return;const id=Date.now().toString(36);profiles[id]={name:name.trim().slice(0,80),...profileFields()};renderProfiles();$('profile').value=id;savePreferences();};
$('profile').onchange=()=>{loadProfile();savePreferences();};
for(const id of ['mode','goal','intention','context']) $(id).onchange=()=>{
  Object.assign(profiles[$('profile').value],profileFields());modeUI();savePreferences();
};
$('settingsToggle').onclick=()=>{$('settings').open=!$('settings').open;$('settingsToggle').setAttribute('aria-expanded',String($('settings').open));if($('settings').open)$('settings').scrollIntoView({behavior:'smooth',block:'start'});};
$('provider').onchange=()=>{providerUI();$('cloudConsent').checked=false;savePreferences();void warmLocalCoach();};
$('ollamaModel').onchange=()=>{savePreferences();void warmLocalCoach();};
$('warmupRetry').onclick=refresh;
window.addEventListener('focus',()=>{void warmLocalCoach();});
setInterval(()=>{void warmLocalCoach();},10*60*1000);$('retain').onchange=()=>{retentionUI();savePreferences();};$('retentionDays').onchange=()=>{retentionUI();savePreferences();};$('timerMinutes').onchange=()=>{timerText();savePreferences();};$('quiet').onchange=savePreferences;$('refresh').onclick=refresh;
$('simulation').onchange=captureNote;
providerUI();retentionUI();timerText();renderVoice();
(async()=>{try{if(window.__TAURI__?.event?.listen) await window.__TAURI__.event.listen('coach-event',coachEvent);await refresh();await refreshVoice();}catch(error){notice(errorMessage(error));}})();
