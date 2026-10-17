'use strict';
(() => {
  const el = id => document.getElementById(id);
  const entries = new Map();
  let epoch = 0, snapshot = null, codex = {busy: false, hasDraft: false, draftText: ''};
  const normalized = text => text.trim().replace(/\s+/g, ' ').toLowerCase();
  const picked = () => [...el('conversationTaskSegments').querySelectorAll('input:checked')].map(input => input.value);
  function sourcesValid() {
    return snapshot && snapshot.epoch === epoch && Date.now() - snapshot.created < 5 * 60 * 1000 &&
      picked().every(id => snapshot.entries.get(id) === entries.get(id));
  }
  function render() {
    el('conversationTaskOpen').disabled = !entries.size || codex.busy;
    if (!snapshot) return;
    const text = el('conversationTaskText').value.trim();
    const valid = sourcesValid();
    el('conversationTaskUse').disabled = codex.busy || !picked().length || !text || text.length > 4000 || !valid;
    el('conversationTaskUse').textContent = codex.hasDraft ? 'Replace task draft' : 'Use as task draft';
    el('conversationTaskReplacement').hidden = !codex.hasDraft;
    el('conversationTaskExisting').textContent = codex.hasDraft ? codex.draftText : '';
    el('conversationTaskStatus').textContent = codex.busy ? 'Wait for Codex to finish, or cancel its task first.'
      : !valid ? 'These excerpts are no longer current. Cancel and choose words again.'
      : !picked().length ? 'No excerpts selected. Choose the words you want to use.'
      : text.length > 4000 ? 'Shorten this draft to 4,000 characters before continuing.'
      : `${text.length} / 4,000 characters · local draft only. Check the wording before continuing.`;
  }
  function close() {
    snapshot = null; el('conversationTaskPanel').hidden = true;
    el('conversationTaskSegments').replaceChildren(); el('conversationTaskText').value = '';
    el('conversationTaskExisting').textContent = ''; el('conversationTaskReplacement').hidden = true;
    el('conversationTaskStatus').textContent = ''; render();
  }
  window.liveCoachConversationTasks = Object.freeze({
    ingest(segment) {
      // Finalized coach-event ingress is the only caller. No speech text is interpreted as a command.
      if (typeof segment.id !== 'string' || !segment.id.trim() || typeof segment.text !== 'string' || !segment.text.trim() || segment.finalized === false || segment.is_final === false) return;
      if (entries.has(segment.id) || [...entries.values()].some(item => normalized(item.text) === normalized(segment.text))) return;
      entries.set(segment.id, Object.freeze({id: segment.id, text: segment.text, speaker: segment.speaker || '', sessionId: segment.session_id}));
      while (entries.size > 100) entries.delete(entries.keys().next().value);
      // New speech cannot mutate the frozen chooser or an edited task draft.
      render();
    },
    discard(id) { entries.delete(id); render(); },
    clear() {
      epoch++; entries.clear(); close();
      window.dispatchEvent(new CustomEvent('live-coach:conversation-cleared'));
    }
  });
  el('conversationTaskOpen').onclick = () => {
    if (!entries.size || codex.busy) return;
    close(); snapshot = {epoch, created: Date.now(), entries: new Map(entries)};
    for (const item of snapshot.entries.values()) {
      const label = document.createElement('label'), input = document.createElement('input'), text = document.createElement('span');
      input.type = 'checkbox'; input.value = item.id;
      text.textContent = `${item.speaker ? item.speaker + ' (check speaker): ' : 'Speaker not verified: '}${item.text}`;
      input.onchange = () => {
        el('conversationTaskText').value = picked().map(id => snapshot.entries.get(id)?.text || '').join('\n\n'); render();
      };
      label.append(input, text); el('conversationTaskSegments').append(label);
    }
    el('conversationTaskPanel').hidden = false; render();
    el('conversationTaskPanel').scrollIntoView({behavior: 'smooth', block: 'nearest'});
  };
  el('conversationTaskText').oninput = render;
  el('conversationTaskCancel').onclick = close;
  el('conversationTaskUse').onclick = () => {
    render();
    if (el('conversationTaskUse').disabled) return;
    const selectedEntries = picked().map(id => snapshot.entries.get(id));
    const sessionId = selectedEntries[0]?.sessionId;
    if (!Number.isSafeInteger(sessionId) || !selectedEntries.every(item => item.sessionId === sessionId)) return;
    const detail = {text: el('conversationTaskText').value.trim(), sessionId, replace: codex.hasDraft, accepted: false};
    window.dispatchEvent(new CustomEvent('live-coach:conversation-task-draft', {detail}));
    if (detail.accepted) close();
    else { render(); el('conversationTaskStatus').textContent = 'Draft could not be transferred. Check Codex status and try again.'; }
  };
  window.addEventListener('live-coach:codex-draft-status', event => { codex = event.detail; render(); });
  // Expiry disables an open chooser even if no further speech arrives.
  setInterval(() => { if (snapshot) render(); }, 1000);
  render();
})();
