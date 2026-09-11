import { UNSUPPORTED } from "../server/read-only-client";
import { isRecord, type LlmHistoryEntry } from "../types";

interface LlmContext {
  llmStatusEl: HTMLElement;
  llmQuestionEl: HTMLInputElement;
  llmAutoCommitEl: HTMLInputElement;
  llmCertifyEl: HTMLInputElement;
  llmVerifyEl: HTMLInputElement;
  llmRequireVerifiedEl: HTMLInputElement;
  llmAskBtn: HTMLButtonElement;
  llmToQueryBtn: HTMLButtonElement;
  llmClearBtn: HTMLButtonElement;
  llmChatEl: HTMLElement;
  llmCitationsEl: HTMLElement;
  llmDebugEl: HTMLElement;
  rerender: () => void;
  clearHighlights: () => void;
  highlightFromToolLoop: (outcome: unknown) => void;
}

export function initLlmTab(ctx: LlmContext) {
  const {
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
    rerender,
    highlightFromToolLoop,
  } = ctx;
  function setLlmStatus(text: string): void {
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
    } catch {
      key = "";
    }
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

  function setLlmHistoryKey(next: string): void {
    llmHistoryKey = next;
  }

  function loadLlmHistoryForKey(key: string): LlmHistoryEntry[] {
    try {
      const raw = localStorage.getItem(key) || "";
      if (!raw.trim()) return [];
      const parsed: unknown = JSON.parse(raw);
      if (!Array.isArray(parsed)) return [];
      return parsed
        .filter(isRecord)
        .map((message) => ({
          role: String(message.role || ""),
          content: String(message.content || ""),
          public_rationale: String(message.public_rationale || ""),
          citations: Array.isArray(message.citations)
            ? message.citations.map((item) => String(item))
            : [],
          queries: Array.isArray(message.queries)
            ? message.queries.map((item) => String(item))
            : [],
          notes: Array.isArray(message.notes)
            ? message.notes.map((item) => String(item))
            : [],
        }))
        .filter((message) => Boolean(message.role && message.content));
    } catch (_e) {
      return [];
    }
  }

  function saveLlmHistory() {
    try {
      localStorage.setItem(llmHistoryKey, JSON.stringify(llmHistory));
    } catch {
      return;
    }
  }

  let llmHistory = loadLlmHistoryForKey(llmHistoryKey); // {role, content}
  function getLlmHistory() {
    return llmHistory;
  }

  function setLlmHistory(next: LlmHistoryEntry[]): void {
    llmHistory = next || [];
  }

  function setLlmCitations(obj: unknown): void {
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

  function openDocChunk(_chunkId: string): void {
    setLlmCitations(UNSUPPORTED);
  }

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
    } catch {
      return;
    }
  }

  function saveLlmCertify() {
    try {
      if (!llmCertifyEl) return;
      localStorage.setItem(
        "axiograph_llm_certify",
        llmCertifyEl.checked ? "1" : "0",
      );
    } catch {
      return;
    }
  }

  function loadLlmVerify() {
    try {
      const v = localStorage.getItem("axiograph_llm_verify") || "";
      if (llmVerifyEl)
        llmVerifyEl.checked =
          v === "1" || v === "true" || v === "yes" || v === "on";
    } catch {
      return;
    }
  }

  function saveLlmVerify() {
    try {
      if (!llmVerifyEl) return;
      localStorage.setItem(
        "axiograph_llm_verify",
        llmVerifyEl.checked ? "1" : "0",
      );
    } catch {
      return;
    }
  }

  function loadLlmRequireVerified() {
    try {
      const v = localStorage.getItem("axiograph_llm_require_verified") || "";
      if (llmRequireVerifiedEl)
        llmRequireVerifiedEl.checked =
          v === "1" || v === "true" || v === "yes" || v === "on";
    } catch {
      return;
    }
  }

  function saveLlmRequireVerified() {
    try {
      if (!llmRequireVerifiedEl) return;
      localStorage.setItem(
        "axiograph_llm_require_verified",
        llmRequireVerifiedEl.checked ? "1" : "0",
      );
    } catch {
      return;
    }
  }

  loadLlmCertify();
  loadLlmVerify();
  loadLlmRequireVerified();
  if (llmCertifyEl) llmCertifyEl.addEventListener("change", saveLlmCertify);
  if (llmVerifyEl) llmVerifyEl.addEventListener("change", saveLlmVerify);
  if (llmRequireVerifiedEl)
    llmRequireVerifiedEl.addEventListener("change", saveLlmRequireVerified);

  function setLlmDebug(obj: unknown): void {
    if (!llmDebugEl) return;
    try {
      llmDebugEl.textContent = obj ? JSON.stringify(obj, null, 2) : "";
    } catch {
      llmDebugEl.textContent = String(obj || "");
    }
  }

  function llmAgentAsk(): void { setLlmStatus(UNSUPPORTED); }
  function llmToQuery(): void { setLlmStatus(UNSUPPORTED); }
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
