/* ============================================================
   LLM Wiki Desktop — Shared App JS
   Inline SVG icon sprite + common interactions.
   ============================================================ */

/* ---------- 1. SVG icon sprite ---------- */
const ICONS = `
<svg xmlns="http://www.w3.org/2000/svg" style="display:none" aria-hidden="true">
<symbol id="i-dashboard" viewBox="0 0 24 24"><rect x="3" y="13" width="4" height="8" rx="1"/><rect x="10" y="8" width="4" height="13" rx="1"/><rect x="17" y="5" width="4" height="16" rx="1"/></symbol>
<symbol id="i-book" viewBox="0 0 24 24"><path d="M3 5h5a4 4 0 0 1 4 4v11a3 3 0 0 0-3-3H3z"/><path d="M21 5h-5a4 4 0 0 0-4 4v11a3 3 0 0 1 3-3h6z"/></symbol>
<symbol id="i-chat" viewBox="0 0 24 24"><path d="M4 5a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H10l-5 4z"/></symbol>
<symbol id="i-graph" viewBox="0 0 24 24"><circle cx="6" cy="6" r="2"/><circle cx="18" cy="6" r="2"/><circle cx="12" cy="18" r="2"/><path d="M8 7l3 9"/><path d="M16 7l-3 9"/><path d="M8 6h8"/></symbol>
<symbol id="i-bot" viewBox="0 0 24 24"><rect x="4" y="8" width="16" height="11" rx="3"/><path d="M12 8V4"/><circle cx="12" cy="3" r="1" fill="currentColor" stroke="none"/><circle cx="9" cy="13" r="1" fill="currentColor" stroke="none"/><circle cx="15" cy="13" r="1" fill="currentColor" stroke="none"/><path d="M9 16h6"/></symbol>
<symbol id="i-upload" viewBox="0 0 24 24"><path d="M12 16V4"/><path d="M7 9l5-5 5 5"/><path d="M4 16v3a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-3"/></symbol>
<symbol id="i-shield" viewBox="0 0 24 24"><path d="M12 3l8 3v6c0 5-3.5 8.5-8 10-4.5-1.5-8-5-8-10V6z"/><path d="M9 12l2 2 4-4"/></symbol>
<symbol id="i-export" viewBox="0 0 24 24"><path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M20 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4"/></symbol>
<symbol id="i-settings" viewBox="0 0 24 24"><path d="M4 6h8M16 6h4"/><circle cx="14" cy="6" r="2"/><path d="M4 12h4M12 12h8"/><circle cx="10" cy="12" r="2"/><path d="M4 18h8M16 18h4"/><circle cx="14" cy="18" r="2"/></symbol>
<symbol id="i-search" viewBox="0 0 24 24"><circle cx="11" cy="11" r="6.5"/><path d="m20 20-4-4"/></symbol>
<symbol id="i-save" viewBox="0 0 24 24"><path d="M5 4h11l3 3v12a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1z"/><rect x="8" y="4" width="8" height="5"/><rect x="8" y="13" width="8" height="6" rx="0.5"/></symbol>
<symbol id="i-edit" viewBox="0 0 24 24"><path d="M16.5 3.5l4 4-11 11H5.5v-4z"/><path d="M14 6l4 4"/></symbol>
<symbol id="i-refresh" viewBox="0 0 24 24"><path d="M3 12a9 9 0 0 1 15.5-6.3L21 8"/><path d="M21 4v4h-4"/><path d="M21 12a9 9 0 0 1-15.5 6.3L3 16"/><path d="M3 20v-4h4"/></symbol>
<symbol id="i-folder" viewBox="0 0 24 24"><path d="M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/></symbol>
<symbol id="i-folder-open" viewBox="0 0 24 24"><path d="M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v1H3z"/><path d="M3 9h18l-2.4 9.5a1 1 0 0 1-1 .5H4.4a1 1 0 0 1-1-.75z"/></symbol>
<symbol id="i-file" viewBox="0 0 24 24"><path d="M6 3h8l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M14 3v5h5"/></symbol>
<symbol id="i-file-md" viewBox="0 0 24 24"><path d="M6 3h8l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M14 3v5h5"/><path d="M9 18v-4l1.5 1.5L12 14v4"/></symbol>
<symbol id="i-diff" viewBox="0 0 24 24"><path d="M12 3v18"/><path d="M5 9h5M5 9l2-2M5 9l2 2"/><path d="M19 15h-5M19 15l-2-2M19 15l-2 2"/></symbol>
<symbol id="i-spinner" viewBox="0 0 24 24"><path d="M12 2v4M12 18v4M4.9 4.9l2.8 2.8M16.3 16.3l2.8 2.8M2 12h4M18 12h4M4.9 19.1l2.8-2.8M16.3 7.7l2.8-2.8"/></symbol>
<symbol id="i-warn" viewBox="0 0 24 24"><path d="M12 3l9 16a1 1 0 0 1-1 1.5H4A1 1 0 0 1 3 19z"/><path d="M12 9v5"/><circle cx="12" cy="17.5" r="0.8" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-err" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M12 7v6"/><circle cx="12" cy="16.5" r="0.8" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-check" viewBox="0 0 24 24"><path d="M4 12l5 5L20 6"/></symbol>
<symbol id="i-check-circle" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M8 12l3 3 5-5"/></symbol>
<symbol id="i-x" viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></symbol>
<symbol id="i-plus" viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></symbol>
<symbol id="i-minus" viewBox="0 0 24 24"><path d="M5 12h14"/></symbol>
<symbol id="i-chev-r" viewBox="0 0 24 24"><path d="M9 6l6 6-6 6"/></symbol>
<symbol id="i-chev-d" viewBox="0 0 24 24"><path d="M6 9l6 6 6-6"/></symbol>
<symbol id="i-chev-l" viewBox="0 0 24 24"><path d="M15 6l-6 6 6 6"/></symbol>
<symbol id="i-chev-u" viewBox="0 0 24 24"><path d="M6 15l6-6 6 6"/></symbol>
<symbol id="i-arrow-r" viewBox="0 0 24 24"><path d="M4 12h16M14 6l6 6-6 6"/></symbol>
<symbol id="i-arrow-u" viewBox="0 0 24 24"><path d="M12 20V4M6 10l6-6 6 6"/></symbol>
<symbol id="i-star" viewBox="0 0 24 24"><path d="M12 3l2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.3 6.8 19l1-5.8-4.2-4.1 5.8-.8z"/></symbol>
<symbol id="i-star-fill" viewBox="0 0 24 24"><path d="M12 3l2.6 5.3 5.8.8-4.2 4.1 1 5.8L12 16.3 6.8 19l1-5.8-4.2-4.1 5.8-.8z" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-tag" viewBox="0 0 24 24"><path d="M3 4v6l11 11 7-7L10 4z"/><circle cx="7" cy="7" r="1.2" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-clock" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></symbol>
<symbol id="i-history" viewBox="0 0 24 24"><path d="M3 5v5h5"/><path d="M3.4 11a9 9 0 1 1 1.4 6.5"/><path d="M12 7v5l3 2"/></symbol>
<symbol id="i-link" viewBox="0 0 24 24"><path d="M9 15l6-6"/><path d="M11 6.5l1-1a4 4 0 0 1 6 6l-1 1"/><path d="M13 17.5l-1 1a4 4 0 0 1-6-6l1-1"/></symbol>
<symbol id="i-external" viewBox="0 0 24 24"><path d="M14 5h5v5"/><path d="M19 5l-9 9"/><path d="M19 13v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h6"/></symbol>
<symbol id="i-copy" viewBox="0 0 24 24"><rect x="9" y="9" width="11" height="11" rx="1.5"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/></symbol>
<symbol id="i-trash" viewBox="0 0 24 24"><path d="M4 7h16M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/><path d="M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"/><path d="M10 11v6M14 11v6"/></symbol>
<symbol id="i-download" viewBox="0 0 24 24"><path d="M12 4v12"/><path d="M7 11l5 5 5-5"/><path d="M4 17v2a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-2"/></symbol>
<symbol id="i-play" viewBox="0 0 24 24"><path d="M7 4l13 8-13 8z"/></symbol>
<symbol id="i-pause" viewBox="0 0 24 24"><rect x="6" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/><rect x="14" y="4" width="4" height="16" rx="1" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-stop" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="1" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-filter" viewBox="0 0 24 24"><path d="M3 5h18l-7 9v6l-4-2v-4z"/></symbol>
<symbol id="i-sort" viewBox="0 0 24 24"><path d="M7 4v12M3 8l4-4 4 4"/><path d="M17 20V8M13 12l4 4 4-4"/></symbol>
<symbol id="i-eye" viewBox="0 0 24 24"><path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7z"/><circle cx="12" cy="12" r="3"/></symbol>
<symbol id="i-pin" viewBox="0 0 24 24"><path d="M9 3h6l-1.5 6 3 3v2h-9v-2l3-3z"/><path d="M12 14v7"/></symbol>
<symbol id="i-key" viewBox="0 0 24 24"><circle cx="7" cy="15" r="3.5"/><path d="M9.5 12.5L20 2"/><path d="M16 6l3 3"/><path d="M19 9l2-2"/></symbol>
<symbol id="i-cpu" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="1"/><rect x="9" y="9" width="6" height="6"/><path d="M9 2v3M15 2v3M9 19v3M15 19v3M2 9h3M2 15h3M19 9h3M19 15h3"/></symbol>
<symbol id="i-globe" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/></symbol>
<symbol id="i-sun" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4"/><path d="M12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1"/></symbol>
<symbol id="i-moon" viewBox="0 0 24 24"><path d="M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8z"/></symbol>
<symbol id="i-bell" viewBox="0 0 24 24"><path d="M6 9a6 6 0 0 1 12 0c0 5 2 7 2 7H4s2-2 2-7z"/><path d="M10.5 19a2 2 0 0 0 3 0"/></symbol>
<symbol id="i-help" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 0 1 5 0c0 2-2.5 2-2.5 4"/><circle cx="12" cy="17" r="0.8" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-list" viewBox="0 0 24 24"><path d="M8 6h12M8 12h12M8 18h12"/><circle cx="4" cy="6" r="1" fill="currentColor" stroke="none"/><circle cx="4" cy="12" r="1" fill="currentColor" stroke="none"/><circle cx="4" cy="18" r="1" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-grid" viewBox="0 0 24 24"><rect x="3" y="3" width="8" height="8" rx="1"/><rect x="13" y="3" width="8" height="8" rx="1"/><rect x="3" y="13" width="8" height="8" rx="1"/><rect x="13" y="13" width="8" height="8" rx="1"/></symbol>
<symbol id="i-menu" viewBox="0 0 24 24"><path d="M3 6h18M3 12h18M3 18h18"/></symbol>
<symbol id="i-more" viewBox="0 0 24 24"><circle cx="5" cy="12" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none"/><circle cx="19" cy="12" r="1.5" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-pin-dot" viewBox="0 0 24 24"><path d="M12 22s7-7 7-12a7 7 0 1 0-14 0c0 5 7 12 7 12z"/><circle cx="12" cy="10" r="2.5"/></symbol>
<symbol id="i-doc" viewBox="0 0 24 24"><path d="M6 3h8l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M14 3v5h5"/><path d="M8 13h8M8 16h8M8 19h5"/></symbol>
<symbol id="i-img" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8" cy="9" r="1.5"/><path d="M21 17l-5-5L3 21"/></symbol>
<symbol id="i-pdf" viewBox="0 0 24 24"><path d="M6 3h8l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1z"/><path d="M14 3v5h5"/><path d="M8 13v6M8 13h1.5a1.5 1.5 0 0 1 0 3H8z"/></symbol>
<symbol id="i-sheet" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="16" rx="1"/><path d="M3 10h18M3 15h18M9 4v16M15 4v16"/></symbol>
<symbol id="i-slide" viewBox="0 0 24 24"><rect x="3" y="4" width="18" height="13" rx="1"/><path d="M8 21h8M12 17v4"/></symbol>
<symbol id="i-link-ext" viewBox="0 0 24 24"><path d="M12 5H5a1 1 0 0 0-1 1v13a1 1 0 0 0 1 1h13a1 1 0 0 0 1-1v-7"/><path d="M14 4h6v6"/><path d="M20 4l-9 9"/></symbol>
<symbol id="i-clipboard" viewBox="0 0 24 24"><path d="M9 4V3a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1h1a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h1z"/><rect x="9" y="3" width="6" height="3" rx="0.5"/></symbol>
<symbol id="i-git" viewBox="0 0 24 24"><circle cx="6" cy="6" r="2"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 8v8"/><path d="M18 10V8a4 4 0 0 0-4-4H8"/></symbol>
<symbol id="i-zap" viewBox="0 0 24 24"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></symbol>
<symbol id="i-info" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/><path d="M12 11v6"/><circle cx="12" cy="7.5" r="0.8" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-wiki" viewBox="0 0 24 24"><path d="M3 5h5a4 4 0 0 1 4 4v11a3 3 0 0 0-3-3H3z"/><path d="M21 5h-5a4 4 0 0 0-4 4v11a3 3 0 0 1 3-3h6z"/><path d="M11 3v3l1-1 1 1V3z" fill="currentColor" stroke="none"/></symbol>
<symbol id="i-folder-add" viewBox="0 0 24 24"><path d="M3 7a1 1 0 0 1 1-1h5l2 2h8a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z"/><path d="M12 11v6M9 14h6"/></symbol>
<symbol id="i-circle" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"/></symbol>
<symbol id="i-corner" viewBox="0 0 24 24"><path d="M14 4h6v6"/><path d="M20 4L8 16"/></symbol>
</svg>
`;

/* ---------- 2. Inject sprite + boot helpers ---------- */
(function () {
  // Inject sprite at the top of body
  function injectIcons() {
    if (document.getElementById('__svg_sprite__')) return;
    const div = document.createElement('div');
    div.id = '__svg_sprite__';
    div.innerHTML = ICONS;
    document.body.insertBefore(div, document.body.firstChild);
  }

  // Helpers
  function icon(name, size) {
    const s = size || 16;
    return `<svg class="ico" width="${s}" height="${s}" aria-hidden="true" focusable="false"><use href="#i-${name}"/></svg>`;
  }
  window.__icon = icon;

  // QSA helper
  function $$(sel, root) { return Array.from((root || document).querySelectorAll(sel)); }
  function $(sel, root) { return (root || document).querySelector(sel); }
  window.__ = { $, $$ };

  // Toggle helper
  function toggle(el, cls) { if (el) el.classList.toggle(cls); }

  // Tree expand/collapse
  function bindTrees() {
    $$('.tree__row[data-toggle]').forEach(function (row) {
      row.addEventListener('click', function (e) {
        if (e.target.closest('[data-stop]')) return;
        row.classList.toggle('is-open');
      });
    });
  }

  // Segmented tabs
  function bindSegs() {
    $$('.seg[data-seg]').forEach(function (seg) {
      seg.querySelectorAll('button').forEach(function (b) {
        b.addEventListener('click', function () {
          seg.querySelectorAll('button').forEach(function (x) { x.classList.remove('is-active'); });
          b.classList.add('is-active');
          const target = seg.getAttribute('data-seg');
          if (target) {
            $$('[data-seg-target]').forEach(function (t) {
              if (t.getAttribute('data-seg-target') === target && t.getAttribute('data-seg-key') === b.getAttribute('data-key')) {
                t.classList.remove('hidden');
              } else if (t.getAttribute('data-seg-target') === target) {
                t.classList.add('hidden');
              }
            });
          }
        });
      });
    });
  }

  // Filter chips toggle
  function bindFilterChips() {
    $$('.pill[data-filter]').forEach(function (chip) {
      chip.addEventListener('click', function () {
        if (chip.getAttribute('data-filter') === 'multi') {
          chip.classList.toggle('pill--active');
        } else {
          const group = chip.getAttribute('data-group') || 'default';
          $$(`.pill[data-group="${group}"]`).forEach(function (x) { x.classList.remove('pill--active'); });
          chip.classList.add('pill--active');
        }
        const target = chip.getAttribute('data-target');
        if (target && chip.getAttribute('data-filter') === 'multi') {
          const tgt = $('#' + target);
          if (tgt) tgt.classList.toggle('hidden');
        }
      });
    });
  }

  // Dialog open/close with focus management + Escape + trap
  function openDialog(overlay, trigger) {
    if (!overlay) return;
    overlay.classList.remove('hidden');
    overlay.__lastFocus = trigger || document.activeElement;
    // role + aria for screen readers
    overlay.setAttribute('aria-hidden', 'false');
    const dlg = overlay.querySelector('.dialog, .drawer');
    if (dlg && !dlg.getAttribute('role')) dlg.setAttribute('role', 'dialog');
    if (dlg && !dlg.getAttribute('aria-modal')) dlg.setAttribute('aria-modal', 'true');
    // focus first focusable inside
    const focusables = dlg ? dlg.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])') : [];
    if (focusables.length) {
      const cancelBtn = Array.prototype.find.call(focusables, function (el) {
        return el.textContent.trim() === '取消' || /close|x/i.test(el.getAttribute('aria-label') || '') || el.matches('[data-dialog-close]');
      });
      setTimeout(function () { (cancelBtn || focusables[0]).focus(); }, 30);
    }
    document.body.style.overflow = 'hidden';
  }
  function closeDialog(overlay) {
    if (!overlay) return;
    overlay.classList.add('hidden');
    overlay.setAttribute('aria-hidden', 'true');
    document.body.style.overflow = '';
    if (overlay.__lastFocus && typeof overlay.__lastFocus.focus === 'function') {
      overlay.__lastFocus.focus();
      overlay.__lastFocus = null;
    }
  }
  function bindDialogs() {
    $$('[data-dialog-open]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const id = btn.getAttribute('data-dialog-open');
        openDialog(document.getElementById(id), btn);
      });
    });
    $$('[data-dialog-close]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        closeDialog(btn.closest('.dialog-overlay, .drawer-overlay'));
      });
    });
    $$('.dialog-overlay, .drawer-overlay').forEach(function (o) {
      o.addEventListener('click', function (e) {
        if (e.target === o) closeDialog(o);
      });
    });
    // Escape key + focus trap
    document.addEventListener('keydown', function (e) {
      const open = document.querySelector('.dialog-overlay:not(.hidden), .drawer-overlay:not(.hidden)');
      if (!open) return;
      if (e.key === 'Escape') { e.preventDefault(); closeDialog(open); return; }
      if (e.key === 'Tab') {
        const dlg = open.querySelector('.dialog, .drawer');
        if (!dlg) return;
        const f = Array.prototype.slice.call(dlg.querySelectorAll('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')).filter(function (el) {
          return el.offsetParent !== null && !el.disabled;
        });
        if (!f.length) return;
        const first = f[0], last = f[f.length - 1];
        if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    });
  }

  // Search filter for [data-search-list] using [data-search-input]
  function bindSearch() {
    $$('[data-search-input]').forEach(function (input) {
      const listSel = input.getAttribute('data-search-input');
      const list = $(listSel);
      if (!list) return;
      input.addEventListener('input', function () {
        const q = input.value.trim().toLowerCase();
        list.querySelectorAll('[data-search-row]').forEach(function (row) {
          const txt = (row.getAttribute('data-search-text') || row.textContent).toLowerCase();
          row.style.display = !q || txt.indexOf(q) >= 0 ? '' : 'none';
        });
        const empty = list.parentNode.querySelector('[data-search-empty]');
        if (empty) {
          const anyVisible = !!list.querySelector('[data-search-row]:not([style*="display: none"])');
          empty.style.display = anyVisible ? 'none' : '';
        }
      });
    });
  }

  // Sidebar collapse
  function bindSidebarToggle() {
    $$('[data-sidebar-toggle]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const app = document.querySelector('.app');
        if (app) app.classList.toggle('sidebar-collapsed');
      });
    });
  }

  // Right panel toggle — wide windows collapse column, narrow windows handled by drawer binding
  function bindRightPanelToggle() {
    $$('[data-right-toggle]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        if (window.innerWidth <= 1180) return; // drawer handler takes over on narrow
        const app = document.querySelector('.app');
        if (app) app.classList.toggle('no-right');
      });
    });
  }

  // Toasts
  window.__toast = function (msg, kind) {
    let stack = document.querySelector('.toast-stack');
    if (!stack) {
      stack = document.createElement('div');
      stack.className = 'toast-stack';
      document.body.appendChild(stack);
    }
    const t = document.createElement('div');
    t.className = 'toast' + (kind ? ' toast--' + kind : '');
    t.innerHTML = (kind === 'ok' ? window.__icon('check') : kind === 'err' ? window.__icon('warn') : window.__icon('info')) + '<span>' + msg + '</span>';
    stack.appendChild(t);
    setTimeout(function () {
      t.style.opacity = '0';
      t.style.transition = 'opacity 200ms';
      setTimeout(function () { t.remove(); }, 220);
    }, 2600);
  };

  // Copy buttons
  function bindCopy() {
    $$('[data-copy]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const sel = btn.getAttribute('data-copy');
        const target = $(sel);
        if (target) {
          const text = target.value || target.textContent;
          if (navigator.clipboard) navigator.clipboard.writeText(text);
          window.__toast('已复制到剪贴板', 'ok');
        }
      });
    });
  }

  // Toggle hidden elements
  function bindToggles() {
    $$('[data-toggle-target]').forEach(function (btn) {
      btn.addEventListener('click', function () {
        const sel = btn.getAttribute('data-toggle-target');
        const el = $(sel);
        if (el) el.classList.toggle('hidden');
      });
    });
  }

  // Auto aria-label from data-tip — keeps tooltips and AT names in sync
  function autoAriaLabels() {
    $$('[data-tip]').forEach(function (el) {
      if (!el.getAttribute('aria-label') && !el.textContent.trim()) {
        el.setAttribute('aria-label', el.getAttribute('data-tip'));
      }
    });
    // SVG icons inside buttons should be hidden from AT
    $$('button .ico, a .ico').forEach(function (svg) {
      if (!svg.getAttribute('aria-hidden')) svg.setAttribute('aria-hidden', 'true');
    });
    // Wire .formrow__label to nearest input/select/textarea via aria-labelledby
    $$('.formrow__label').forEach(function (label, i) {
      const row = label.closest('.formrow');
      if (!row) return;
      const control = row.querySelector('.formrow__control');
      if (!control) return;
      const field = control.querySelector('input, select, textarea');
      if (!field) return;
      if (!field.id) field.id = 'auto-field-' + i;
      if (!label.id) label.id = 'auto-label-' + i;
      if (!field.getAttribute('aria-labelledby')) field.setAttribute('aria-labelledby', label.id);
    });
    // Group checkboxes inside a control get group label from preceding strong/label inside same row
    $$('.formrow input[type="checkbox"]').forEach(function (cb) {
      if (!cb.id && !cb.getAttribute('aria-label')) {
        const txt = (cb.parentElement && cb.parentElement.textContent || '').trim();
        if (txt) cb.setAttribute('aria-label', txt.slice(0, 80));
      }
    });
    // Language switch buttons get role="group" container
    const ls = $('.langswitch');
    if (ls && !ls.getAttribute('role')) {
      ls.setAttribute('role', 'group');
      ls.setAttribute('aria-label', '语言切换');
    }
  }

  // Inject skip-link for keyboard users
  function injectSkipLink() {
    if (document.querySelector('.skip-link')) return;
    const main = document.querySelector('main.main, main, [role="main"]');
    if (!main) return;
    if (!main.id) main.id = 'main-content';
    const link = document.createElement('a');
    link.href = '#' + main.id;
    link.className = 'skip-link';
    link.textContent = '跳到主内容';
    document.body.insertBefore(link, document.body.firstChild);
  }

  // On narrow windows, right panel toggle opens a drawer instead of removing column
  function bindResponsiveRightPanel() {
    const app = document.querySelector('.app');
    if (!app) return;
    function syncDrawerState() {
      if (window.innerWidth > 1180) {
        app.classList.remove('rightpanel-open');
      }
    }
    window.addEventListener('resize', syncDrawerState);
    syncDrawerState();
    // Any element with [data-right-toggle] becomes an open/close for the drawer on narrow windows
    document.addEventListener('click', function (e) {
      const trigger = e.target.closest('[data-right-toggle]');
      if (!trigger) return;
      if (window.innerWidth <= 1180) {
        e.stopPropagation();
        app.classList.toggle('rightpanel-open');
        return;
      }
      // Wide window: legacy no-right toggle still applies
    });
    // Click outside the drawer (on app background) closes it
    document.addEventListener('click', function (e) {
      if (!app.classList.contains('rightpanel-open')) return;
      const rp = app.querySelector('.rightpanel');
      if (!rp) return;
      if (!rp.contains(e.target) && !e.target.closest('[data-right-toggle]')) {
        app.classList.remove('rightpanel-open');
      }
    });
    // Escape closes drawer
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && app.classList.contains('rightpanel-open')) {
        app.classList.remove('rightpanel-open');
      }
    });
  }

  // Boot
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', boot);
  } else { boot(); }
  function boot() {
    injectIcons();
    injectSkipLink();
    autoAriaLabels();
    bindTrees();
    bindSegs();
    bindFilterChips();
    bindDialogs();
    bindSearch();
    bindSidebarToggle();
    bindRightPanelToggle();
    bindResponsiveRightPanel();
    bindCopy();
    bindToggles();
  }
})();
