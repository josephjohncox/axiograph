// @ts-nocheck

export async function initServerControls(ctx) {
  const {
    serverControlsEl,
    graph,
    ui,
    selectedIdRef,
    setLlmStatus,
    setWorldModelStatus,
    llmHistoryStorageKey,
    loadLlmHistoryForKey,
    renderLlmChat,
    draftOverlayStorageKey,
    loadDraftOverlayForKey,
    saveDraftOverlay,
    renderDraftOverlayReview,
    getLlmHistory,
    setLlmHistory,
    getLlmHistoryKey,
    setLlmHistoryKey,
    getDraftOverlayKey,
    setDraftOverlayKey,
    pickRandomComponentNode,
    llmStatusEl,
    wmStatusEl,
  } = ctx;

  if (!serverControlsEl) return;
  const proto = window.location && window.location.protocol;
  if (!(proto === "http:" || proto === "https:")) return;

  try {
    const [statusResp, snapsResp, ctxResp] = await Promise.all([
      fetch("/status", { cache: "no-store" }),
      fetch("/snapshots", { cache: "no-store" }),
      fetch("/contexts", { cache: "no-store" }),
    ]);
    if (!statusResp.ok || !snapsResp.ok) return;
    const status = await statusResp.json();
    const snaps = await snapsResp.json();
    const ctxsPayload = ctxResp && ctxResp.ok ? await ctxResp.json() : null;
    try {
      const accepted = status && status.snapshot && status.snapshot.accepted_snapshot_id ? String(status.snapshot.accepted_snapshot_id) : "";
      if (accepted) localStorage.setItem("axiograph_server_accepted_snapshot_id", accepted);
    } catch (_e) {}
    // If the accepted snapshot id became available, migrate chat history keys so
    // conversations don't "disappear" on the next reload.
    try {
      const nextKey = llmHistoryStorageKey();
      const curKey = getLlmHistoryKey();
      if (nextKey && nextKey !== curKey) {
        const nextHistory = loadLlmHistoryForKey(nextKey);
        const curHistory = getLlmHistory();
        if (nextHistory.length) {
          setLlmHistory(nextHistory);
        } else if (curHistory.length) {
          localStorage.setItem(nextKey, JSON.stringify(curHistory));
        }
        setLlmHistoryKey(nextKey);
        renderLlmChat();
      }
    } catch (_e) {}
    // Same for draft overlays (review/commit workflow).
    try {
      const nextKey = draftOverlayStorageKey();
      const curKey = getDraftOverlayKey();
      if (nextKey && nextKey !== curKey) {
        const nextOverlay = loadDraftOverlayForKey(nextKey);
        if (nextOverlay && nextOverlay.proposals_json) {
          ui.draftOverlay = nextOverlay;
          if (Array.isArray(nextOverlay.proposals_json.proposals)) {
            ui.draftSelected = new Set(nextOverlay.proposals_json.proposals.map(p => String(p && p.proposal_id || "")).filter(Boolean));
          } else {
            ui.draftSelected = new Set();
          }
        } else if (ui.draftOverlay) {
          localStorage.setItem(nextKey, JSON.stringify(ui.draftOverlay));
        }
        setDraftOverlayKey(nextKey);
        saveDraftOverlay();
        renderDraftOverlayReview();
      }
    } catch (_e) {}
    if (status && status.llm && llmStatusEl) {
      if (status.llm.enabled) setLlmStatus(`ready (${status.llm.backend})`);
      else setLlmStatus("disabled");
    }
    if (status && status.world_model && wmStatusEl) {
      if (status.world_model.enabled) {
        setWorldModelStatus(`ready (${status.world_model.backend})`);
      } else {
        setWorldModelStatus("disabled");
      }
    }
    const list = Array.isArray(snaps.snapshots) ? snaps.snapshots : [];
    if (!list.length) return;

    const layer = snaps.layer || "";
    const statusSnap = (layer === "pathdb")
      ? (status.snapshot && status.snapshot.pathdb_snapshot_id)
      : (status.snapshot && status.snapshot.accepted_snapshot_id);

    const params = new URLSearchParams(window.location.search || "");
    const cur = params.get("snapshot") || statusSnap || list[0].snapshot_id;
    const idx = list.findIndex(s => s.snapshot_id === cur);

    function gotoSnapshot(idOrNull) {
      const p = new URLSearchParams(window.location.search || "");
      if (!idOrNull) p.delete("snapshot");
      else p.set("snapshot", idOrNull);
      window.location.search = p.toString();
    }

    const label = document.createElement("span");
    label.textContent = "snapshot:";

    const headBtn = document.createElement("button");
    headBtn.className = "btn";
    headBtn.textContent = "Head";
    headBtn.addEventListener("click", () => gotoSnapshot(null));

    const newerBtn = document.createElement("button");
    newerBtn.className = "btn";
    newerBtn.textContent = "Newer";
    newerBtn.disabled = !(idx > 0);
    newerBtn.addEventListener("click", () => {
      if (idx > 0) gotoSnapshot(list[idx - 1].snapshot_id);
    });

    const olderBtn = document.createElement("button");
    olderBtn.className = "btn";
    olderBtn.textContent = "Older";
    olderBtn.disabled = !(idx >= 0 && idx + 1 < list.length);
    olderBtn.addEventListener("click", () => {
      if (idx >= 0 && idx + 1 < list.length) gotoSnapshot(list[idx + 1].snapshot_id);
    });

    const sel = document.createElement("select");
    for (const s of list) {
      const opt = document.createElement("option");
      const msg = s.message ? ` — ${s.message}` : "";
      opt.value = s.snapshot_id;
      opt.textContent = `${s.snapshot_id}${msg}`;
      sel.appendChild(opt);
    }
    sel.value = cur;
    sel.addEventListener("change", () => gotoSnapshot(sel.value));

    const info = document.createElement("span");
    info.className = "muted";
    info.textContent = `layer=${layer} snapshots=${list.length}`;

    serverControlsEl.innerHTML = "";
    const snapshotRow = document.createElement("div");
    snapshotRow.className = "server-row";
    snapshotRow.appendChild(label);
    snapshotRow.appendChild(headBtn);
    snapshotRow.appendChild(newerBtn);
    snapshotRow.appendChild(olderBtn);
    snapshotRow.appendChild(sel);
    snapshotRow.appendChild(info);
    serverControlsEl.appendChild(snapshotRow);

    const params2 = new URLSearchParams(window.location.search || "");
    const curFocusName = params2.get("focus_name") || "";
    const curFocusId = params2.get("focus_id") || "";
    const isAll = ["1", "true", "yes", "on"].includes(String(params2.get("all") || "").toLowerCase());

    function setParams(next) {
      const p = new URLSearchParams(window.location.search || "");
      for (const [k, v] of Object.entries(next)) {
        if (v === null || v === undefined || v === "") p.delete(k);
        else p.set(k, String(v));
      }
      window.location.search = p.toString();
    }

    const focusLabel = document.createElement("span");
    focusLabel.textContent = "focus:";

    const focusInput = document.createElement("input");
    focusInput.placeholder = "name";
    focusInput.value = curFocusName || "";

    const focusIdInput = document.createElement("input");
    focusIdInput.placeholder = "id";
    focusIdInput.value = curFocusId || "";
    focusIdInput.style.width = "90px";

    const focusNameBtn = document.createElement("button");
    focusNameBtn.className = "btn";
    focusNameBtn.textContent = "go name";
    focusNameBtn.addEventListener("click", () => {
      const v = (focusInput.value || "").trim();
      if (!v) return;
      setParams({ focus_name: v, focus_id: null, all: null });
    });

    const focusIdBtn = document.createElement("button");
    focusIdBtn.className = "btn";
    focusIdBtn.textContent = "go id";
    focusIdBtn.addEventListener("click", () => {
      const v = (focusIdInput.value || "").trim();
      if (!v) return;
      setParams({ focus_id: v, focus_name: null, all: null });
    });

    const focusSelectedBtn = document.createElement("button");
    focusSelectedBtn.className = "btn";
    focusSelectedBtn.textContent = "focus selected";
    focusSelectedBtn.addEventListener("click", () => {
      const selectedId = selectedIdRef();
      if (selectedId == null) return;
      setParams({ focus_id: selectedId, focus_name: null, all: null });
    });

    const allBtn = document.createElement("button");
    allBtn.className = "btn";
    allBtn.textContent = isAll ? "all ✓" : "all";
    allBtn.addEventListener("click", () => {
      setParams({ all: "1", focus_name: null, focus_id: null });
    });

    const neighBtn = document.createElement("button");
    neighBtn.className = "btn";
    neighBtn.textContent = "neighborhood";
    neighBtn.disabled = !isAll;
    neighBtn.addEventListener("click", () => {
      setParams({ all: null });
    });

    const compCount = ui.components ? ui.components.length : 0;
    const compBtn = document.createElement("button");
    compBtn.className = "btn";
    compBtn.textContent = compCount > 1 ? `other component (${compCount})` : "other component";
    compBtn.disabled = compCount < 2;
    compBtn.title = compCount < 2
      ? "No other components (try all view)."
      : "Jump to a different connected component.";
    compBtn.addEventListener("click", () => {
      let focus = null;
      if (curFocusId) {
        const n = Number(curFocusId);
        if (Number.isFinite(n)) focus = n;
      }
      const selectedId = selectedIdRef();
      if (focus == null && selectedId != null) focus = selectedId;
      if (focus == null && graph.summary && Array.isArray(graph.summary.focus_ids) && graph.summary.focus_ids.length) {
        const n = Number(graph.summary.focus_ids[0]);
        if (Number.isFinite(n)) focus = n;
      }
      const nextId = pickRandomComponentNode(focus);
      if (nextId == null) return;
      setParams({ focus_id: nextId, focus_name: null, all: null });
    });

    const ctxSelect = document.createElement("select");
    const ctxLabel = document.createElement("span");
    ctxLabel.textContent = "context:";
    const ctxOpt = document.createElement("option");
    ctxOpt.value = "";
    ctxOpt.textContent = "(focus)";
    ctxSelect.appendChild(ctxOpt);
    const ctxs = (ctxsPayload && Array.isArray(ctxsPayload.contexts))
      ? ctxsPayload.contexts
      : (Array.isArray(graph.contexts) ? graph.contexts : []);
    for (const ctx of ctxs) {
      const opt = document.createElement("option");
      opt.value = String(ctx.id);
      opt.textContent = ctx.name ? `${ctx.name} (#${ctx.id})` : `Context#${ctx.id}`;
      ctxSelect.appendChild(opt);
    }
    ctxSelect.addEventListener("change", () => {
      const v = String(ctxSelect.value || "");
      if (!v) return;
      setParams({ focus_id: v, focus_name: null, all: null });
    });

    const focusRow = document.createElement("div");
    focusRow.className = "server-row";
    focusRow.appendChild(focusLabel);
    focusRow.appendChild(focusInput);
    focusRow.appendChild(focusNameBtn);
    focusRow.appendChild(focusIdInput);
    focusRow.appendChild(focusIdBtn);
    focusRow.appendChild(focusSelectedBtn);
    focusRow.appendChild(allBtn);
    focusRow.appendChild(neighBtn);
    focusRow.appendChild(compBtn);
    if (ctxs.length) {
      focusRow.appendChild(ctxLabel);
      focusRow.appendChild(ctxSelect);
    }
    serverControlsEl.appendChild(focusRow);
  } catch (_e) {
    // ignore: offline or no server endpoints
  }
}
