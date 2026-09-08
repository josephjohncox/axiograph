// @ts-nocheck

import { UNSUPPORTED } from "../server/read-only-client";

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
    // Retain the existing local history key for inspection only. Cached legacy
    // snapshot strings are not authenticated identity and are never migrated
    // from server status or used in requests.
    const host =
      window.location && window.location.host
        ? window.location.host
        : "offline";
    let key = "";
    try {
      const accepted = (
        localStorage.getItem("axiograph_server_accepted_snapshot_id") || ""
      ).trim();
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
      const raw = localStorage.getItem(key) || "";
      if (!raw.trim()) return [];
      const v = JSON.parse(raw);
      if (!Array.isArray(v)) return [];
      return v
        .map((m) => ({
          role: String((m && m.role) || ""),
          content: String((m && m.content) || ""),
          public_rationale: String((m && m.public_rationale) || ""),
          citations: Array.isArray(m && m.citations)
            ? m.citations.map((x) => String(x))
            : [],
          queries: Array.isArray(m && m.queries)
            ? m.queries.map((x) => String(x))
            : [],
          notes: Array.isArray(m && m.notes)
            ? m.notes.map((x) => String(x))
            : [],
        }))
        .filter((m) => m.role && m.content);
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

  async function openDocChunk() { setLlmCitations(UNSUPPORTED); }

  function renderLlmChat() {
    if (!llmChatEl) return;
    llmChatEl.replaceChildren();
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

      const rationale =
        m && typeof m.public_rationale === "string"
          ? m.public_rationale.trim()
          : "";
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

      const citations = Array.isArray(m.citations)
        ? m.citations.filter(Boolean)
        : [];
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
          openBtn.title = UNSUPPORTED;
          openBtn.disabled = true;
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
      citations: Array.isArray(e.citations)
        ? e.citations.map((x) => String(x))
        : [],
      queries: Array.isArray(e.queries) ? e.queries.map((x) => String(x)) : [],
      notes: Array.isArray(e.notes) ? e.notes.map((x) => String(x)) : [],
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

  // The read-only server has no mutation endpoint; persisted preferences cannot
  // enable auto-commit or turn generated evidence into accepted state.
  if (llmAutoCommitEl) {
    llmAutoCommitEl.checked = false;
    llmAutoCommitEl.disabled = true;
    llmAutoCommitEl.title =
      "Auto-commit unavailable: review generated evidence locally; use the canonical check/authoring and AxiStore CLI workflow.";
  }

  function loadLlmCertify() {
    try {
      const v = localStorage.getItem("axiograph_llm_certify") || "";
      if (llmCertifyEl)
        llmCertifyEl.checked =
          v === "1" || v === "true" || v === "yes" || v === "on";
    } catch (_e) {}
  }

  function saveLlmCertify() {
    try {
      if (!llmCertifyEl) return;
      localStorage.setItem(
        "axiograph_llm_certify",
        llmCertifyEl.checked ? "1" : "0",
      );
    } catch (_e) {}
  }

  function loadLlmVerify() {
    try {
      const v = localStorage.getItem("axiograph_llm_verify") || "";
      if (llmVerifyEl)
        llmVerifyEl.checked =
          v === "1" || v === "true" || v === "yes" || v === "on";
    } catch (_e) {}
  }

  function saveLlmVerify() {
    try {
      if (!llmVerifyEl) return;
      localStorage.setItem(
        "axiograph_llm_verify",
        llmVerifyEl.checked ? "1" : "0",
      );
    } catch (_e) {}
  }

  function loadLlmRequireVerified() {
    try {
      const v = localStorage.getItem("axiograph_llm_require_verified") || "";
      if (llmRequireVerifiedEl)
        llmRequireVerifiedEl.checked =
          v === "1" || v === "true" || v === "yes" || v === "on";
    } catch (_e) {}
  }

  function saveLlmRequireVerified() {
    try {
      if (!llmRequireVerifiedEl) return;
      localStorage.setItem(
        "axiograph_llm_require_verified",
        llmRequireVerifiedEl.checked ? "1" : "0",
      );
    } catch (_e) {}
  }

  loadLlmCertify();
  loadLlmVerify();
  loadLlmRequireVerified();
  if (llmCertifyEl) llmCertifyEl.addEventListener("change", saveLlmCertify);
  if (llmVerifyEl) llmVerifyEl.addEventListener("change", saveLlmVerify);
  if (llmRequireVerifiedEl)
    llmRequireVerifiedEl.addEventListener("change", saveLlmRequireVerified);

  function setLlmDebug(obj) {
    if (!llmDebugEl) return;
    try {
      llmDebugEl.textContent = obj ? JSON.stringify(obj, null, 2) : "";
    } catch {
      llmDebugEl.textContent = String(obj || "");
    }
  }

  async function llmAgentAsk() { setLlmStatus(UNSUPPORTED); }
  async function llmToQuery() { setLlmStatus(UNSUPPORTED); }
  for (const control of [llmQuestionEl, llmAskBtn, llmToQueryBtn, llmCertifyEl, llmVerifyEl, llmRequireVerifiedEl]) {
    if (control) { control.disabled = true; control.title = UNSUPPORTED; }
  }
  setLlmStatus(UNSUPPORTED);
  if (llmAskBtn) llmAskBtn.addEventListener("click", llmAgentAsk);
  if (llmToQueryBtn) llmToQueryBtn.addEventListener("click", llmToQuery);
  if (llmQuestionEl)
    llmQuestionEl.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter") {
        ev.preventDefault();
        llmAgentAsk();
      }
    });
  if (llmClearBtn)
    llmClearBtn.addEventListener("click", () => {
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
    openDocChunk,
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
