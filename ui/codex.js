'use strict';
(() => {
  const el = id => document.getElementById(id);
  const rpc = (command, args = {}) => window.__TAURI__?.core?.invoke
    ? window.__TAURI__.core.invoke(command, args)
    : Promise.reject(new Error('Open Live Coach as a macOS application to connect Codex.'));
  let connected = false, project = null, review = null, runId = null, busy = false;
  let revision = 0, connection = 0, unlisten = null, pendingEvents = [], sending = false;
  let historySnapshot = null, historyCursor = null, historyRevision = 0;
  const approvals = new Map(), seenApprovals = new Set();
  const scope = () => el('codexMode').value === 'plan'
    ? 'Codex can read the chosen project and propose a plan. Changes are blocked.'
    : 'Codex can read and change files in the chosen project. Commands may request your approval.';
  const message = value => { el('codexStatus').textContent = value; };
  const error = value => message(typeof value === 'string' ? value : value?.message || String(value));
  const selected = () => [...el('codexFiles').querySelectorAll('input:checked')].map(input => input.value);
  function render() {
    el('codexConnect').hidden = connected;
    el('codexConnect').disabled = busy;
    el('codexDisconnect').hidden = !connected;
    el('codexSetup').hidden = !connected;
    el('codexReview').hidden = !review;
    el('codexSend').disabled = !review || busy || !!runId;
    el('codexPrepare').disabled = !connected || !project || !el('codexPrompt').value.trim() || !el('codexModel').value || busy || !!runId;
    el('codexCancel').disabled = !runId;
    for (const input of el('codexSetup').querySelectorAll('input,select,textarea,button')) {
      if (input.id !== 'codexPrepare') input.disabled = busy || !!runId;
    }
    el('codexHistoryMore').hidden = !historyCursor;
    el('codexHistoryProjectOnly').disabled = busy || !!runId || !project;
    el('codexScope').textContent = scope();
  }
  function invalidate() {
    revision++; review = null; el('codexReviewText').textContent = ''; render();
  }
  const selectedHistory = () => [...el('codexHistoryMessages').querySelectorAll('input:checked')].map(input => input.value);
  function clearHistory() {
    historyRevision++; historySnapshot = null; historyCursor = null;
    el('codexHistoryMessages').replaceChildren(); el('codexHistoryThreads').replaceChildren();
    el('codexHistoryPreview').hidden = true; el('codexHistoryTitle').textContent = '';
    el('codexHistoryStatus').textContent = ''; el('codexHistory').hidden = true;
  }
  function showProject(result) {
    project = result.project; el('codexProjectPath').value = project;
    el('codexFiles').replaceChildren();
    for (const file of result.files || []) {
      const label = document.createElement('label'), input = document.createElement('input'), text = document.createElement('span');
      input.type = 'checkbox'; input.value = file.path; input.onchange = invalidate;
      text.textContent = `${file.path} (${file.bytes} bytes)`; label.append(input, text); el('codexFiles').append(label);
    }
    el('codexProjectStatus').textContent = `${project} · ${result.files.length} files available.${result.truncated ? ' File list limited; choose a narrower project if needed.' : ''} Choose only what this task needs.`;
    message('Project loaded. Choose files and describe your task.');
  }
  async function loadProject(path, choose = false) {
    invalidate(); clearHistory(); project = null; el('codexFiles').replaceChildren(); el('codexProjectStatus').textContent = ''; busy = true;
    const generation = connection, version = revision; render();
    try {
      const result = await rpc(choose ? 'codex_choose_project' : 'codex_project', choose ? {} : {path});
      if (generation !== connection || version !== revision || !connected) return;
      if (result) showProject(result);
      else message('Folder selection cancelled. Choose a project to continue.');
    } catch (e) { if (generation === connection && version === revision) error(e); }
    finally { if (generation === connection) { busy = false; render(); } }
  }
  async function readHistory(thread) {
    invalidate(); historySnapshot = null; el('codexHistoryMessages').replaceChildren(); el('codexHistoryPreview').hidden = true;
    const version = ++historyRevision, generation = connection; busy = true; render();
    try {
      const result = await rpc('codex_history_read', {threadId: thread.id});
      if (generation !== connection || version !== historyRevision || !connected) return;
      historySnapshot = result.snapshot_id; el('codexHistoryTitle').textContent = result.title;
      for (const item of result.messages || []) {
        const label = document.createElement('label'), input = document.createElement('input'), text = document.createElement('pre');
        input.type = 'checkbox'; input.value = item.id; input.onchange = invalidate;
        text.textContent = `${item.role}: ${item.text}`; label.append(input, text); el('codexHistoryMessages').append(label);
      }
      el('codexHistoryPreview').hidden = false;
      el('codexHistoryStatus').textContent = result.truncated ? 'Conversation preview limited. Only the messages shown can be selected.' : 'Preview loaded. Choose individual messages to include.';
    } catch (e) { if (generation === connection && version === historyRevision) error(e); }
    finally { if (generation === connection) { busy = false; render(); } }
  }
  async function listHistory(more = false) {
    invalidate(); historySnapshot = null; el('codexHistoryMessages').replaceChildren(); el('codexHistoryPreview').hidden = true;
    const cursor = more ? historyCursor : null;
    if (!more) el('codexHistoryThreads').replaceChildren();
    historyCursor = null; el('codexHistory').hidden = false;
    const version = ++historyRevision, generation = connection; busy = true; render();
    el('codexHistoryStatus').textContent = 'Loading conversations…';
    try {
      const result = await rpc('codex_history_list', {search: el('codexHistorySearch').value.trim() || null, cursor, project: el('codexHistoryProjectOnly').checked ? project : null});
      if (generation !== connection || version !== historyRevision || !connected) return;
      historyCursor = result.next_cursor;
      for (const thread of result.threads || []) {
        const card = document.createElement('section'), title = document.createElement('p'), details = document.createElement('p'), preview = document.createElement('button');
        title.textContent = thread.title; const timestamp = thread.updated_at;
        const date = new Date(timestamp == null || timestamp === '' ? NaN : typeof timestamp === 'number' ? timestamp * 1000 : timestamp);
        details.textContent = [thread.project, Number.isFinite(date.getTime()) ? date.toLocaleString() : null].filter(Boolean).join(' · ');
        preview.textContent = 'Preview messages'; preview.onclick = () => readHistory(thread); card.append(title, details, preview);
        if (thread.project) { const choose = document.createElement('button'); choose.textContent = 'Use this project'; choose.onclick = () => loadProject(thread.project); card.append(choose); }
        el('codexHistoryThreads').append(card);
      }
      el('codexHistoryStatus').textContent = el('codexHistoryThreads').childElementCount ? 'Preview a conversation or choose its project. Existing conversations are never resumed.' : 'No conversations found.';
    } catch (e) { if (generation === connection && version === historyRevision) { el('codexHistoryStatus').textContent = 'History could not be loaded. Try again.'; error(e); } }
    finally { if (generation === connection) { busy = false; render(); } }
  }
  function append(text) {
    const out = el('codexOutput');
    out.textContent = (out.textContent + String(text)).slice(-64000);
    out.scrollTop = out.scrollHeight;
  }
  function clearApprovals() { approvals.clear(); seenApprovals.clear(); el('codexApprovals').replaceChildren(); }
  function event(data) {
    if (data.type === 'connection' && data.connected === false) {
      ++connection; connected = false; review = null; runId = null;
      sending = false; pendingEvents = []; project = null; busy = false;
      clearApprovals(); clearHistory(); el('codexFiles').replaceChildren(); el('codexReviewText').textContent = '';
      render(); message(data.text || 'Codex disconnected. Connect again to continue.'); return;
    }
    if (sending && !runId) { if (pendingEvents.length < 100) pendingEvents.push(data); return; }
    if (!connected || !runId || data.run_id !== runId) return;
    if (data.type === 'approval_resolved' && data.approval_id) {
      approvals.get(data.approval_id)?.remove(); approvals.delete(data.approval_id); seenApprovals.add(data.approval_id);
    }
    if (data.type === 'output') append(data.text || '');
    if (data.type === 'status') message(data.text || data.status || 'Codex is working.');
    if (data.type === 'approval' && data.approval_id && !seenApprovals.has(data.approval_id)) {
      const card = document.createElement('section'); card.className = 'codex-approval';
      const title = document.createElement('h3'); title.textContent = 'Codex requests approval';
      const details = document.createElement('pre'); details.textContent = [data.method, data.details].filter(Boolean).join('\n');
      card.append(title, details);
      if (data.can_accept === false) { const explanation = document.createElement('p'); explanation.textContent = 'This request cannot be approved here. Decline or cancel this task.'; card.append(explanation); }
      approvals.set(data.approval_id, card); seenApprovals.add(data.approval_id);
      for (const [label, decision] of [['Allow once', 'accept'], ['Decline', 'decline'], ['Cancel task', 'cancel']]) {
        const button = document.createElement('button'); button.textContent = label; button.dataset.decision = decision;
        if (decision === 'accept' && data.can_accept === false) { button.disabled = true; button.title = 'This request cannot be approved here. Decline or cancel this task.'; }
        button.onclick = async () => {
          if (decision === 'accept' && data.can_accept === false) return;
          if (!approvals.has(data.approval_id) || runId !== data.run_id) return;
          card.querySelectorAll('button').forEach(b => { b.disabled = true; });
          try {
            await rpc('codex_approval', {runId: data.run_id, approvalId: data.approval_id, decision});
            approvals.delete(data.approval_id); card.remove();
            if (decision === 'cancel') message('Cancellation requested.');
          } catch (e) { error(e); if (approvals.has(data.approval_id)) card.querySelectorAll('button').forEach(b => { b.disabled = b.dataset.decision === 'accept' && data.can_accept === false; }); }
        };
        card.append(button);
      }
      el('codexApprovals').append(card);
    }
    if (data.type === 'completed' || data.type === 'error') {
      if (data.text) append('\n' + data.text + '\n');
      message(data.type === 'error' ? data.text || 'Codex task failed.' : data.text || 'Codex task completed.');
      runId = null; review = null; clearApprovals(); render();
    }
  }
  el('codexConnect').onclick = async () => {
    busy = true; const generation = ++connection; render(); message('Connecting to Codex…');
    try {
      if (!window.__TAURI__?.event?.listen) throw new Error('Codex events are unavailable.');
      if (!unlisten) unlisten = await window.__TAURI__.event.listen('codex-event', e => event(e.payload));
      const result = await rpc('codex_connect');
      if (generation !== connection) return;
      connected = result.connected === true;
      el('codexModel').replaceChildren();
      for (const model of result.models || []) { const option = document.createElement('option'); option.value = model.id; option.textContent = model.name || model.id; el('codexModel').append(option); }
      message(result.message || 'Connected. Choose a project folder.');
    } catch (e) { error(e); } finally { busy = false; render(); }
  };
  el('codexLoadProject').onclick = () => loadProject(el('codexProjectPath').value.trim());
  el('codexChooseProject').onclick = () => loadProject(null, true);
  el('codexBrowseHistory').onclick = () => listHistory();
  el('codexHistorySearchButton').onclick = () => listHistory();
  el('codexHistoryMore').onclick = () => listHistory(true);
  el('codexHistoryProjectOnly').onchange = () => listHistory();
  el('codexHistorySearch').oninput = () => { historyRevision++; historySnapshot = null; historyCursor = null; el('codexHistoryPreview').hidden = true; el('codexHistoryMessages').replaceChildren(); el('codexHistoryThreads').replaceChildren(); el('codexHistoryStatus').textContent = 'Search to load matching conversations.'; invalidate(); };
  el('codexHistorySearch').onkeydown = e => { if (e.key === 'Enter' && !busy && !runId) { e.preventDefault(); listHistory(); } };
  el('codexHistoryClear').onclick = () => { historyRevision++; historySnapshot = null; el('codexHistoryMessages').replaceChildren(); el('codexHistoryPreview').hidden = true; invalidate(); };
  el('codexProjectPath').oninput = () => { clearHistory(); project = null; el('codexFiles').replaceChildren(); el('codexProjectStatus').textContent = ''; invalidate(); };
  for (const id of ['codexModel','codexMode','codexPrompt']) el(id).addEventListener('input', invalidate);
  for (const id of ['codexModel','codexMode']) el(id).addEventListener('change', invalidate);
  el('codexPrepare').onclick = async () => {
    invalidate(); const version = revision, generation = connection;
    busy = true; render();
    try {
      const result = await rpc('codex_prepare', {project, files: selected(), task: el('codexPrompt').value.trim(), model: el('codexModel').value, mode: el('codexMode').value, historySnapshot: selectedHistory().length ? historySnapshot : null, historyItems: selectedHistory()});
      if (version !== revision || generation !== connection || !connected) return;
      review = result;
      el('codexReviewText').textContent = [
        `Project: ${result.project}`, `Model: ${result.model}`, `Provider: ${result.provider}`,
        `Access: ${result.mode}`, `Scope: ${result.scope}`, '\nTask:', result.task,
        '\nSelected project context:', ...(result.files || []).map(f => `\n--- ${f.path} ---\n${f.text}`),
        '\nSelected Codex history:', ...(result.history || []).map(item => `\n--- ${item.title || item.thread_id || 'Conversation'} · ${item.role} ---\n${item.text}`),
        '\nComplete prompt sent to Codex:', result.prompt
      ].join('\n');
      message('Review the task, access and project text before sending.');
    } catch (e) { error(e); } finally { busy = false; render(); }
  };
  el('codexSend').onclick = async () => {
    if (!review || busy || runId) return;
    const token = review.review_id, generation = connection;
    busy = true; sending = true; pendingEvents = []; el('codexOutput').textContent = ''; clearApprovals(); render();
    try {
      const result = await rpc('codex_send', {reviewId: token});
      if (generation !== connection) return;
      runId = result.run_id; review = null; el('codexRun').hidden = false;
      sending = false; const events = pendingEvents; pendingEvents = []; events.forEach(event);
      if (runId) message('Task sent to Codex.');
    } catch (e) { review = null; error(e); } finally { sending = false; pendingEvents = []; busy = false; render(); }
  };
  el('codexCancel').onclick = async () => {
    if (!runId) return;
    const target = runId; el('codexCancel').disabled = true;
    try { await rpc('codex_cancel', {runId: target}); if (runId === target) { runId = null; review = null; clearApprovals(); message('Task cancelled.'); } }
    catch (e) { error(e); } finally { render(); }
  };
  el('codexDisconnect').onclick = async () => {
    ++connection; invalidate(); connected = false; busy = false; runId = null; sending = false; pendingEvents = []; clearApprovals();
    project = null; clearHistory(); el('codexFiles').replaceChildren(); el('codexRun').hidden = true; el('codexOutput').textContent = ''; render();
    try { await rpc('codex_disconnect'); message('Disconnected.'); } catch (e) { error(e); }
    if (typeof unlisten === 'function') unlisten(); unlisten = null;
  };
  render();
})();
