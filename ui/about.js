// About and update dialog.
import { $, appVersion, errToast, invoke, listen, setStatusText, settings, t, toast } from "./core.js";
import { fillUpdateIntervalSelect } from "./settingsPanel.js";
import { attachSelectPicker } from "./selectpop.js";

export function bindAbout() {
  // Clicking the app logo opens the project's GitHub page (drag still moves the window).
  $("app-logo")?.addEventListener("click", () => invoke("open_repo").catch((e) => errToast(e)));

  // About dialog: version + project page link.
  const aboutEl = $("about-modal");
  const closeAbout = () => aboutEl.classList.add("hidden");
  $("about-btn").addEventListener("click", () => aboutEl.classList.remove("hidden"));
  $("about-close").addEventListener("click", closeAbout);
  $("about-ok").addEventListener("click", closeAbout);
  $("about-repo").addEventListener("click", () => invoke("open_repo").catch((e) => errToast(e)));
  aboutEl.addEventListener("pointerdown", (e) => {
    if (e.target === aboutEl) closeAbout();
  });

  // Updates: manual check in the settings + daily background notification.
  let updateInfo = null;
  const updateBtn = $("update-btn");
  const updateVersion = $("update-version");
  // The status line (left of the button) is the version by default, and switches to
  // "Update to X available" once an update is found (manual OR auto). The button text
  // never changes: it always offers a fresh manual check.
  const offerUpdate = (info) => {
    updateInfo = info;
    setStatusText(updateVersion, "updateAvailableBtn", "v", info.version);
    updateVersion.classList.add("update-avail");
    // Undo a previous "up to date" check's tooltip override, if any.
    updateVersion.dataset.i18nTitle = "openReleases";
    updateVersion.title = t("openReleases");
  };
  // Clicking the status line: open the update dialog if one is pending, else the
  // GitHub releases page.
  updateVersion.addEventListener("click", () => {
    if (updateInfo?.available) openUpdateModal(updateInfo);
    else invoke("open_repo", { page: "releases" }).catch((e) => errToast(e));
  });

  // Changelog popup shown before any install (manual check or daily toast).
  const updateModal = {
    root: $("update-modal"),
    title: $("update-modal-title"),
    notes: $("update-modal-notes"),
    warn: $("update-modal-warn"),
    cancel: $("update-modal-cancel"),
    skip: $("update-modal-skip"),
    install: $("update-modal-install"),
    refresh: $("update-modal-refresh"),
    close: $("update-modal-close"),
  };
  // The release body arrives as Markdown. Render the part release notes actually use
  // (headings, bullets, tables, bold and code) instead of dumping the raw source
  // with its "###" and "**" into a monospace box. Everything is built node by node:
  // the text comes from the network and must never reach innerHTML.
  const renderNotes = (box, text, version) => {
    box.textContent = "";
    const inline = (target, s) => {
      // A link has nowhere to go inside a dialog, so only its label survives.
      const re = /\*\*(.+?)\*\*|`([^`]+?)`|\[(.+?)\]\([^)]*\)/g;
      let last = 0, m;
      while ((m = re.exec(s))) {
        if (m.index > last) target.append(s.slice(last, m.index));
        if (m[3] !== undefined) target.append(m[3]);
        else {
          const el = document.createElement(m[1] !== undefined ? "strong" : "code");
          el.textContent = m[1] ?? m[2];
          target.append(el);
        }
        last = re.lastIndex;
      }
      target.append(s.slice(last));
    };
    let list = null, table = null, para = null;
    for (const raw of text.replace(/\r/g, "").split("\n")) {
      const line = raw.trim();
      if (!line) { list = table = para = null; continue; }
      const head = /^#{1,6}\s+(.*)$/.exec(line);
      const item = /^[-*+]\s+(.*)$/.exec(line);
      const row = line.startsWith("|") && line.endsWith("|");
      if (!item) list = null;
      if (!row) table = null;
      if (head || item || row) para = null;
      if (head) {
        // The dialog title already names the version, so its heading would only repeat it.
        if (version && head[1].includes(version)) continue;
        const h = document.createElement("h3");
        h.textContent = head[1];
        box.append(h);
      } else if (item) {
        if (!list) { list = document.createElement("ul"); box.append(list); }
        const li = document.createElement("li");
        inline(li, item[1]);
        list.append(li);
      } else if (row) {
        const cells = line.slice(1, -1).split("|").map((c) => c.trim());
        if (cells.every((c) => /^:?-{2,}:?$/.test(c))) continue; // the |---|---| divider
        if (!table) { table = document.createElement("table"); box.append(table); }
        const tr = document.createElement("tr");
        for (const c of cells) {
          const cell = document.createElement(table.rows.length ? "td" : "th");
          inline(cell, c);
          tr.append(cell);
        }
        table.append(tr);
      } else {
        // Markdown wraps a paragraph over several lines; keep it as one.
        if (para) para.append(" ");
        else { para = document.createElement("p"); box.append(para); }
        inline(para, line);
      }
    }
  };

  // The same dialog also shows the shipped changelog ("what's new"): no install or
  // skip then, a refresh from GitHub instead, and Close returns to About when About
  // was where it came from.
  let notesSince = null; // version boundary the open list was computed from (null: current version only)
  let notesFromAbout = false;
  const setUpdateButtons = (notes) => {
    updateModal.skip.classList.toggle("hidden", notes);
    updateModal.install.classList.toggle("hidden", notes);
    updateModal.refresh.classList.toggle("hidden", !notes);
    updateModal.cancel.dataset.i18n = notes ? "close" : "cancel";
    updateModal.cancel.textContent = t(notes ? "close" : "cancel");
  };
  const openUpdateModal = (info) => {
    offerUpdate(info);
    notesFromAbout = false;
    setUpdateButtons(false);
    updateModal.title.textContent = t("updateAvailable").replace("{v}", () => info.version);
    renderNotes(updateModal.notes, info.notes?.trim() || t("noChangelog"), info.version || "");
    updateModal.warn.classList.toggle("hidden", !info.skipped);
    updateModal.root.classList.remove("hidden");
  };
  const openNotesInUpdateModal = (sections, since, fromAbout) => {
    notesSince = since ?? null;
    notesFromAbout = !!fromAbout;
    setUpdateButtons(true);
    updateModal.title.textContent = t("whatsNewTitle");
    updateModal.warn.classList.add("hidden");
    const combined = (sections || []).map((s) => s.markdown).join("\n\n").trim();
    // Every heading kept (no `version` passed to renderNotes): several
    // sections need their own "## x.y.z" to stay tellable apart.
    renderNotes(updateModal.notes, combined || t("noChangelog"), "");
    updateModal.root.classList.remove("hidden");
  };
  const closeUpdateModal = () => {
    updateModal.root.classList.add("hidden");
    if (notesFromAbout) aboutEl.classList.remove("hidden");
    notesFromAbout = false;
  };

  updateBtn.addEventListener("click", async () => {
    updateBtn.disabled = true;
    try {
      const info = await invoke("check_update");
      if (info.available) offerUpdate(info);          // status → "Update to X available"
      else {
        // The short form keeps this single-line at the same width the long
        // sentence used to wrap at; the full sentence survives as a tooltip.
        // dataset.i18nTitle (not just .title) keeps it correct across a later
        // language switch too, the same as every other data-i18n-title element.
        setStatusText(updateVersion, "upToDate", "v", appVersion);
        updateVersion.dataset.i18nTitle = "upToDateTip";
        updateVersion.title = t("upToDateTip");
        updateVersion.classList.remove("update-avail");
      }
    } catch (err) {
      errToast(err);
    }
    updateBtn.disabled = false;
  });
  // No launch check here: the backend schedules the automatic check itself (a couple
  // of seconds after start, then at the chosen interval) and announces what it finds
  // through "update-available". It does nothing while Auto update is off.
  updateModal.install.addEventListener("click", async () => {
    updateModal.install.disabled = true;
    try {
      await invoke("install_update", { url: updateInfo.url }); // app exits
    } catch (err) {
      updateModal.install.disabled = false;
      // A portable copy is not an error: the release page opened instead.
      if (String(err) === "portable-update-manual") {
        closeUpdateModal();
        toast(t("errUpdatePortableManual"));
        return;
      }
      errToast(err);
    }
  });
  updateModal.skip.addEventListener("click", async () => {
    const v = updateInfo?.version;
    if (!v) return;
    try { await invoke("skip_version", { version: v }); } catch (err) { errToast(err); }
    updateInfo = null;
    setStatusText(updateVersion, "versionInstalled", "v", appVersion);
    updateVersion.classList.remove("update-avail");
    closeUpdateModal();
    toast(t("versionSkipped").replace("{v}", () => v));
  });
  updateModal.cancel.addEventListener("click", closeUpdateModal);
  updateModal.close.addEventListener("click", closeUpdateModal);
  updateModal.root.addEventListener("pointerdown", (e) => {
    if (e.target === updateModal.root) closeUpdateModal();
  });

  // The interval beside the Auto update switch only shows while the switch is on.
  $("opt-autoupdate").addEventListener("change", (e) => {
    settings.auto_update = e.target.checked;
    fillUpdateIntervalSelect();
    invoke("set_auto_update", { enabled: e.target.checked }).catch((err) => errToast(err));
  });
  attachSelectPicker($("opt-update-interval"));
  $("opt-update-interval").addEventListener("change", async (e) => {
    const hours = Number(e.target.value);
    settings.ui_values = { ...(settings.ui_values || {}), updateCheckH: hours };
    await invoke("set_ui_value", { key: "updateCheckH", value: hours }).catch((err) => errToast(err));
  });
  listen("update-available", (e) => {
    offerUpdate(e.payload);
    const icon =
      '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 16V4"/><path d="M7 9l5-5 5 5"/><path d="M5 20h14"/></svg>';
    toast(t("updateAvailable").replace("{v}", () => e.payload.version), {
      label: t("installNow"),
      variant: "update",
      icon,
      onClick: () => openUpdateModal(e.payload),
    });
  });

  // ---------- what's new ----------
  updateModal.refresh.addEventListener("click", async () => {
    updateModal.refresh.disabled = true;
    try {
      const sections = await invoke("whats_new_refresh", { since: notesSince });
      const combined = (sections || []).map((s) => s.markdown).join("\n\n").trim();
      renderNotes(updateModal.notes, combined || t("noChangelog"), "");
    } catch (err) {
      toast(t("whatsNewRefreshFailed"));
    }
    updateModal.refresh.disabled = false;
  });
  $("about-whats-new").addEventListener("click", async () => {
    try {
      const sections = await invoke("whats_new_current");
      closeAbout();
      openNotesInUpdateModal(sections, null, true);
    } catch (err) {
      errToast(err);
    }
  });
  // Startup check runs unconditionally (it is what marks the version seen) and
  // the notice after an update always shows once.
  invoke("whats_new_check")
    .then((result) => {
      if (!result?.sections?.length) return;
      toast(t("whatsNewToast"), {
        label: t("whatsNewBtn"),
        onClick: () => openNotesInUpdateModal(result.sections, result.since ?? null, false),
      });
    })
    .catch(() => {});

  // ---------- third-party licenses ----------
  // The list itself comes from src-tauri/build.rs (generated at build time from the
  // real dependency manifests); this only renders whatever it returns. Fetched once
  // and cached, since the list cannot change while the app is running.
  let thirdPartyCache = null;
  const licensesView = {
    search: $("licenses-search"),
    list: $("licenses-list"),
    empty: $("licenses-empty"),
  };
  const renderLicenseEntries = (data) => {
    licensesView.list.textContent = "";
    const entries = data.entries || [];
    licensesView.search.classList.toggle("hidden", entries.length <= 20);
    for (const entry of entries) {
      const row = document.createElement("div");
      row.className = "license-row";
      row.dataset.searchKey = `${entry.name} ${entry.license}`.toLowerCase();

      const toggle = document.createElement("button");
      toggle.type = "button";
      toggle.className = "license-row-toggle";
      toggle.setAttribute("aria-expanded", "false");
      const nameSpan = document.createElement("span");
      nameSpan.className = "license-row-name";
      nameSpan.textContent = `${entry.name} ${entry.version}`;
      const licenseSpan = document.createElement("span");
      licenseSpan.className = "license-row-license";
      licenseSpan.textContent = entry.license;
      toggle.append(nameSpan, licenseSpan);

      const body = document.createElement("div");
      body.className = "license-row-body hidden";
      if (entry.note) {
        const note = document.createElement("p");
        note.className = "license-row-note";
        note.textContent = entry.note;
        body.appendChild(note);
      }
      const text = entry.licenseId ? data.licenseTexts[entry.licenseId] : null;
      if (text) {
        const pre = document.createElement("pre");
        pre.className = "license-row-text";
        pre.textContent = text;
        body.appendChild(pre);
      } else if (!entry.note) {
        // Public-domain style entries with neither a note nor a license text
        // still get something visible on expand -- never a blank panel.
        const p = document.createElement("p");
        p.className = "license-row-note";
        p.textContent = entry.license;
        body.appendChild(p);
      }

      toggle.addEventListener("click", () => {
        const open = toggle.getAttribute("aria-expanded") === "true";
        toggle.setAttribute("aria-expanded", String(!open));
        body.classList.toggle("hidden", open);
      });

      row.append(toggle, body);
      licensesView.list.appendChild(row);
    }
    filterLicenseRows("");
  };
  const filterLicenseRows = (query) => {
    const q = query.trim().toLowerCase();
    let visible = 0;
    licensesView.list.querySelectorAll(".license-row").forEach((row) => {
      const match = !q || row.dataset.searchKey.includes(q);
      row.classList.toggle("hidden", !match);
      if (match) visible++;
    });
    licensesView.empty.classList.toggle("hidden", visible !== 0);
  };
  licensesView.search.addEventListener("input", () => filterLicenseRows(licensesView.search.value));
  // The list expands in place inside About; the dialog widens while it is open.
  $("about-licenses").addEventListener("click", async () => {
    const body = $("about-licenses-body");
    const open = body.classList.contains("hidden");
    body.classList.toggle("hidden", !open);
    $("about-licenses").setAttribute("aria-expanded", String(open));
    aboutEl.querySelector(".about-box").classList.toggle("licenses-open", open);
    if (!open) return;
    try {
      if (!thirdPartyCache) {
        thirdPartyCache = JSON.parse(await invoke("third_party_notices"));
      }
      renderLicenseEntries(thirdPartyCache);
    } catch (err) {
      errToast(err);
    }
  });
  return { closeUpdateModal };
}
