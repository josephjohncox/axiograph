// @ts-nocheck

export function initLlmTab(ctx) {
  const {
    ui,
    llmStatusEl,
    llmQuestionEl,
    llmAutoCommitEl,
    llmCertifyEl,
    llmVerifyEl,
    llmRequireVerifiedEl,
    llmAskBtn,
    llmToQueryBtn,
    llmClearBtn,
    llmChatEl,
    llmCitationsEl,
    llmDebugEl,
    axqlQueryEl,
    addMessageEl,
    addAdminTokenEl,
    selectedContextFilter,
    setActiveTab,
    setAxqlStatus,
    rerender,
    prefillAddFromToolLoop,
    clearHighlights,
    highlightFromToolLoop,
  } = ctx;
function setLlmStatus(text) {
  if (!llmStatusEl) return;
  llmStatusEl.textContent = text || "";
}

function llmHistoryStorageKey() {
  // Persist chat per *accepted snapshot* when available (stable across WAL
  // commits), falling back to the current snapshot param. This keeps
  // conversation context durable across “add → commit → reload”.
  const host = (window.location && window.location.host) ? window.location.host : "offline";
  let key = "";
  try {
    const accepted = (localStorage.getItem("axiograph_server_accepted_snapshot_id") || "").trim();
    if (accepted) key = accepted;
  } catch (_e) {}
  if (!key) {
    const params = new URLSearchParams(window.location.search || "");
    key = (params.get("snapshot") || "").trim();
  }
  if (!key) key = "default";
  return `axiograph_llm_history_v2:${host}:${key}`;
}

let llmHistoryKey = llmHistoryStorageKey();
function getLlmHistoryKey() {
  return llmHistoryKey;
}

function setLlmHistoryKey(next) {
  llmHistoryKey = next;
}

function loadLlmHistoryForKey(key) {
  try {
    let raw = localStorage.getItem(key) || "";
    // Backward-compat migration from v1 (host-scoped) history.
    if (!raw.trim()) {
      const host = (window.location && window.location.host) ? window.location.host : "offline";
      raw = localStorage.getItem(`axiograph_llm_history_v1:${host}`) || "";
    }
    if (!raw.trim()) return [];
    const v = JSON.parse(raw);
    if (!Array.isArray(v)) return [];
    return v
      .map(m => ({
        role: String(m && m.role || ""),
        content: String(m && m.content || ""),
        public_rationale: String(m && m.public_rationale || ""),
        citations: Array.isArray(m && m.citations) ? m.citations.map(x => String(x)) : [],
        queries: Array.isArray(m && m.queries) ? m.queries.map(x => String(x)) : [],
        notes: Array.isArray(m && m.notes) ? m.notes.map(x => String(x)) : []
      }))
      .filter(m => m.role && m.content);
  } catch (_e) {
    return [];
  }
}

function saveLlmHistory() {
  try {
    localStorage.setItem(llmHistoryKey, JSON.stringify(llmHistory));
  } catch (_e) {}
}

let llmHistory = loadLlmHistoryForKey(llmHistoryKey); // {role, content}
function getLlmHistory() {
  return llmHistory;
}

function setLlmHistory(next) {
  llmHistory = next || [];
}

function setLlmCitations(obj) {
  if (!llmCitationsEl) return;
  if (obj == null) {
    llmCitationsEl.textContent = "";
    return;
  }
  try {
    if (typeof obj === "string") llmCitationsEl.textContent = obj;
    else llmCitationsEl.textContent = JSON.stringify(obj, null, 2);
  } catch {
    llmCitationsEl.textContent = String(obj);
  }
}

async function openDocChunk(chunkId) {
  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) {
    setLlmCitations("DocChunk lookup requires server mode (open via `axiograph db serve`).");
    return;
  }

  const chunk_id = String(chunkId || "").trim();
  if (!chunk_id) return;
  setLlmCitations(`loading ${chunk_id}…`);

  try {
    const params = new URLSearchParams(window.location.search || "");
    const snapshot = params.get("snapshot");
    const q = new URLSearchParams();
    q.set("chunk_id", chunk_id);
    q.set("max_chars", "4000");
    if (snapshot) q.set("snapshot", snapshot);

    const resp = await fetch(`/docchunk/get?${q.toString()}`);
    const data = await resp.json();
    if (!resp.ok) {
      setLlmCitations(data);
      return;
    }
    setLlmCitations(data);

    const id = data && data.result && typeof data.result.id === "number" ? data.result.id : null;
    if (typeof id === "number") {
      ui.highlightIds = new Set([id]);
      rerender();
    }
  } catch (e) {
    setLlmCitations(String(e));
  }
}

function renderLlmChat() {
  if (!llmChatEl) return;
  llmChatEl.innerHTML = "";
  if (!llmHistory.length) {
    const empty = document.createElement("div");
    empty.className = "muted";
    empty.textContent = "Ask a question to start a conversation.";
    llmChatEl.appendChild(empty);
    return;
  }
  for (const m of llmHistory) {
    const box = document.createElement("div");
    box.className = `llmmsg ${m.role || "unknown"}`;
    const role = document.createElement("div");
    role.className = "role";
    role.textContent = (m.role || "unknown") + ":";
    const text = document.createElement("div");
    text.className = "text";
    text.textContent = String(m.content || "");
    box.appendChild(role);
    box.appendChild(text);

    const rationale = (m && typeof m.public_rationale === "string") ? m.public_rationale.trim() : "";
    if (rationale) {
      const det = document.createElement("details");
      det.style.marginTop = "6px";
      const sum = document.createElement("summary");
      sum.className = "muted";
      sum.textContent = "rationale";
      const pre = document.createElement("pre");
      pre.style.whiteSpace = "pre-wrap";
      pre.style.maxHeight = "160px";
      pre.style.overflow = "auto";
      pre.style.margin = "6px 0 0 0";
      pre.textContent = rationale;
      det.appendChild(sum);
      det.appendChild(pre);
      box.appendChild(det);
    }

    const citations = Array.isArray(m.citations) ? m.citations.filter(Boolean) : [];
    if (citations.length) {
      const citeBox = document.createElement("div");
      citeBox.className = "muted";
      citeBox.style.marginTop = "6px";
      citeBox.textContent = `citations (${citations.length}): `;
      for (const c of citations.slice(0, 8)) {
        const chunkId = String(c);
        const openBtn = document.createElement("button");
        openBtn.type = "button";
        openBtn.className = "btn";
        openBtn.style.padding = "2px 8px";
        openBtn.style.marginLeft = "6px";
        openBtn.textContent = chunkId;
        openBtn.title = "Open DocChunk";
        openBtn.addEventListener("click", () => openDocChunk(chunkId));
        citeBox.appendChild(openBtn);
      }
      if (citations.length > 8) {
        const more = document.createElement("span");
        more.className = "muted";
        more.textContent = ` +${citations.length - 8} more`;
        citeBox.appendChild(more);
      }
      box.appendChild(citeBox);
    }

    const queries = Array.isArray(m.queries) ? m.queries.filter(Boolean) : [];
    if (queries.length) {
      const det = document.createElement("details");
      det.style.marginTop = "6px";
      const sum = document.createElement("summary");
      sum.className = "muted";
      sum.textContent = `queries (${queries.length})`;
      const pre = document.createElement("pre");
      pre.style.whiteSpace = "pre-wrap";
      pre.style.maxHeight = "200px";
      pre.style.overflow = "auto";
      pre.style.margin = "6px 0 0 0";
      pre.textContent = queries.join("\n");
      det.appendChild(sum);
      det.appendChild(pre);
      box.appendChild(det);
    }
    llmChatEl.appendChild(box);
  }
  llmChatEl.scrollTop = llmChatEl.scrollHeight;
}

function appendLlmMessage(role, content, extras) {
  const e = extras && typeof extras === "object" ? extras : {};
  llmHistory.push({
    role,
    content: String(content || ""),
    public_rationale: String(e.public_rationale || ""),
    citations: Array.isArray(e.citations) ? e.citations.map(x => String(x)) : [],
    queries: Array.isArray(e.queries) ? e.queries.map(x => String(x)) : [],
    notes: Array.isArray(e.notes) ? e.notes.map(x => String(x)) : []
  });
  if (llmHistory.length > 40) llmHistory.splice(0, llmHistory.length - 40);
  // Keep localStorage bounded too (models sometimes emit long answers).
  for (const m of llmHistory) {
    if (m && typeof m.content === "string" && m.content.length > 2800) {
      m.content = m.content.slice(0, 2800) + "…";
    }
  }
  saveLlmHistory();
  renderLlmChat();
}

renderLlmChat();

function loadLlmAutoCommit() {
  try {
    const v = localStorage.getItem("axiograph_llm_auto_commit") || "";
    if (llmAutoCommitEl) llmAutoCommitEl.checked = (v === "1" || v === "true" || v === "yes" || v === "on");
  } catch (_e) {}
}

function saveLlmAutoCommit() {
  try {
    if (!llmAutoCommitEl) return;
    localStorage.setItem("axiograph_llm_auto_commit", llmAutoCommitEl.checked ? "1" : "0");
  } catch (_e) {}
}

loadLlmAutoCommit();
if (llmAutoCommitEl) llmAutoCommitEl.addEventListener("change", saveLlmAutoCommit);

function loadLlmCertify() {
  try {
    const v = localStorage.getItem("axiograph_llm_certify") || "";
    if (llmCertifyEl) llmCertifyEl.checked = (v === "1" || v === "true" || v === "yes" || v === "on");
  } catch (_e) {}
}

function saveLlmCertify() {
  try {
    if (!llmCertifyEl) return;
    localStorage.setItem("axiograph_llm_certify", llmCertifyEl.checked ? "1" : "0");
  } catch (_e) {}
}

function loadLlmVerify() {
  try {
    const v = localStorage.getItem("axiograph_llm_verify") || "";
    if (llmVerifyEl) llmVerifyEl.checked = (v === "1" || v === "true" || v === "yes" || v === "on");
  } catch (_e) {}
}

function saveLlmVerify() {
  try {
    if (!llmVerifyEl) return;
    localStorage.setItem("axiograph_llm_verify", llmVerifyEl.checked ? "1" : "0");
  } catch (_e) {}
}

function loadLlmRequireVerified() {
  try {
    const v = localStorage.getItem("axiograph_llm_require_verified") || "";
    if (llmRequireVerifiedEl) llmRequireVerifiedEl.checked = (v === "1" || v === "true" || v === "yes" || v === "on");
  } catch (_e) {}
}

function saveLlmRequireVerified() {
  try {
    if (!llmRequireVerifiedEl) return;
    localStorage.setItem("axiograph_llm_require_verified", llmRequireVerifiedEl.checked ? "1" : "0");
  } catch (_e) {}
}

loadLlmCertify();
loadLlmVerify();
loadLlmRequireVerified();
if (llmCertifyEl) llmCertifyEl.addEventListener("change", saveLlmCertify);
if (llmVerifyEl) llmVerifyEl.addEventListener("change", saveLlmVerify);
if (llmRequireVerifiedEl) llmRequireVerifiedEl.addEventListener("change", saveLlmRequireVerified);

function setLlmDebug(obj) {
  if (!llmDebugEl) return;
  try {
    llmDebugEl.textContent = obj ? JSON.stringify(obj, null, 2) : "";
  } catch {
    llmDebugEl.textContent = String(obj || "");
  }
}


async function llmAgentAsk() {
  if (!llmQuestionEl) return;
  const q = String(llmQuestionEl.value || "").trim();
  if (!q) return;

  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) {
    setLlmStatus("LLM requires server mode (open via `axiograph db serve`).");
    return;
  }

  // Send a bounded amount of prior conversation as context (assistant text is
  // untrusted; the tool-loop should validate via tools).
  const historyToSend = llmHistory.slice(-40).map(m => ({
    role: String(m && m.role || ""),
    content: String(m && m.content || "")
  }));
  appendLlmMessage("user", q);
  setLlmStatus("asking…");
  setLlmDebug(null);
  setLlmCitations(null);

    const ctx = selectedContextFilter();
    const contexts = [];
    if (ctx !== "*" && ctx !== "__none__") contexts.push(ctx);

  try {
    const params = new URLSearchParams(window.location.search || "");
    const snapshot = params.get("snapshot");
    const wantsAutoCommit = llmAutoCommitEl && llmAutoCommitEl.checked;
    const token = (addAdminTokenEl && addAdminTokenEl.value || "").trim();

    const body = { question: q, contexts, history: historyToSend };
    if (snapshot) body.snapshot = snapshot;
    if (llmCertifyEl && llmCertifyEl.checked) body.certify_queries = true;
    if (llmVerifyEl && llmVerifyEl.checked) {
      body.verify_queries = true;
      body.certify_queries = true;
    }
    if (llmRequireVerifiedEl && llmRequireVerifiedEl.checked) {
      body.require_verified_queries = true;
      body.verify_queries = true;
      body.certify_queries = true;
    }
    if (wantsAutoCommit) {
      body.auto_commit = true;
      if (addMessageEl && String(addMessageEl.value || "").trim()) {
        body.commit_message = String(addMessageEl.value || "").trim();
      } else {
        body.commit_message = `llm: ${q}`;
      }
    }

    const headers = { "content-type": "application/json" };
    if (token) headers["authorization"] = `Bearer ${token}`;

    const resp = await fetch("/llm/agent", {
      method: "POST",
      headers,
      body: JSON.stringify(body)
    });
    const data = await resp.json();
    if (!resp.ok) {
      setLlmStatus(`error (${resp.status})`);
      setLlmDebug(data);
      appendLlmMessage("assistant", (data && data.error) ? data.error : "LLM request failed.");
      return;
    }
    setLlmStatus("ok");
    setLlmDebug(data);
    const outcome = data && data.outcome ? data.outcome : null;
    if (outcome && outcome.final_answer && outcome.final_answer.answer) {
      appendLlmMessage(
        "assistant",
        outcome.final_answer.answer,
        {
          public_rationale: outcome.final_answer.public_rationale || "",
          citations: outcome.final_answer.citations || [],
          queries: outcome.final_answer.queries || [],
          notes: outcome.final_answer.notes || []
        }
      );
    } else {
      appendLlmMessage("assistant", "No answer.");
    }
    const didPrefill = prefillAddFromToolLoop(outcome);
    if (didPrefill) {
      if (wantsAutoCommit) {
        const c = data && data.commit ? data.commit : null;
        if (c && c.ok && c.snapshot_id) {
          appendLlmMessage("assistant", `Committed WAL snapshot ${c.snapshot_id}. Reloading…`);
          const p = new URLSearchParams(window.location.search || "");
          p.set("snapshot", c.snapshot_id);
          window.location.search = p.toString();
          return;
        }
        if (c && c.error) {
          appendLlmMessage("assistant", `Auto-commit failed: ${c.error}`);
        } else {
          appendLlmMessage("assistant", "Auto-commit did not apply any changes (no generated overlay).");
        }
      } else {
        appendLlmMessage("assistant", "Review the generated overlay in the Add tab, then click commit to apply it (requires master/admin token).");
      }
    }
    ctx.clearHighlights();
    highlightFromToolLoop(outcome);
    rerender();
  } catch (e) {
    setLlmStatus("error");
    appendLlmMessage("assistant", String(e));
  }
}

if (llmAskBtn) llmAskBtn.addEventListener("click", llmAgentAsk);
async function llmToQuery() {
  if (!llmQuestionEl || !axqlQueryEl) return;
  const q = String(llmQuestionEl.value || "").trim();
  if (!q) return;

  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) {
    setLlmStatus("LLM requires server mode (open via `axiograph db serve`).");
    return;
  }

  setLlmStatus("to_query…");
  setLlmDebug(null);
  try {
    const params = new URLSearchParams(window.location.search || "");
    const snapshot = params.get("snapshot");
    const body = { question: q };
    if (snapshot) body.snapshot = snapshot;

    const resp = await fetch("/llm/to_query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    });
    const data = await resp.json();
    if (!resp.ok) {
      setLlmStatus(`error (${resp.status})`);
      setLlmDebug(data);
      return;
    }
    if (data && typeof data.axql === "string" && data.axql.trim()) {
      axqlQueryEl.value = data.axql;
      setAxqlStatus("filled from llm");
      setActiveTab("query");
    } else {
      setAxqlStatus("LLM returned no query");
    }
    setLlmStatus("ok");
    setLlmDebug(data);
  } catch (e) {
    setLlmStatus("error");
    setLlmDebug(String(e));
  }
}
if (llmToQueryBtn) llmToQueryBtn.addEventListener("click", llmToQuery);
if (llmQuestionEl) llmQuestionEl.addEventListener("keydown", (ev) => {
  if (ev.key === "Enter") {
    ev.preventDefault();
    llmAgentAsk();
  }
});
if (llmClearBtn) llmClearBtn.addEventListener("click", () => {
  setLlmStatus("");
  llmHistory.splice(0, llmHistory.length);
  saveLlmHistory();
  renderLlmChat();
  setLlmDebug(null);
  setLlmCitations(null);
  ctx.clearHighlights();
  rerender();
});


  return {
    setLlmStatus,
    renderLlmChat,
    llmHistoryStorageKey,
    loadLlmHistoryForKey,
    saveLlmHistory,
    getLlmHistory,
    setLlmHistory,
    getLlmHistoryKey,
    setLlmHistoryKey,
    setLlmDebug,
    setLlmCitations,
    highlightFromToolLoop,
  };
}
