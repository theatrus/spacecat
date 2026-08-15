//! The hub's management page: one inline HTML document, no build pipeline.
//!
//! The page drives the JSON API under `/api` with the session cookie and
//! CSRF token from `/api/session`. Channels and roles come from Discord via
//! `/api/guilds/{id}/options` — nobody types an ID anywhere.

pub const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Chatstronomy hub</title>
<style>
  :root {
    --bg: #0b0e14; --panel: #141922; --panel2: #1a2130; --border: #2a3245;
    --text: #e6edf3; --muted: #8b96a8; --accent: #58a6ff; --accent2: #1f6feb;
    --good: #3fb950; --bad: #f85149; --warn: #d29922; --radius: 10px;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; background: var(--bg); color: var(--text);
    font: 15px/1.55 system-ui, -apple-system, "Segoe UI", sans-serif;
  }
  .wrap { max-width: 860px; margin: 0 auto; padding: 1.5rem 1rem 4rem; }
  header { display: flex; align-items: center; gap: .75rem; margin-bottom: 1.25rem; }
  header h1 { font-size: 1.25rem; margin: 0; flex: 1; letter-spacing: .01em; }
  header h1 span { color: var(--muted); font-weight: normal; font-size: .9rem; }
  .avatar { width: 28px; height: 28px; border-radius: 50%; vertical-align: middle; }
  a { color: var(--accent); text-decoration: none; }
  a:hover { text-decoration: underline; }

  .card {
    background: var(--panel); border: 1px solid var(--border);
    border-radius: var(--radius); padding: 1.1rem 1.2rem; margin-bottom: 1.1rem;
  }
  .card > .head { display: flex; align-items: center; gap: .6rem; flex-wrap: wrap; }
  .card h2 { margin: 0; font-size: 1.05rem; flex: 1; }
  .sub {
    background: var(--panel2); border: 1px solid var(--border);
    border-radius: var(--radius); padding: .9rem 1rem; margin-top: .9rem;
  }
  .sub .head { display: flex; align-items: center; gap: .6rem; margin-bottom: .6rem; }
  .sub .head b { font-size: 1rem; }
  .grow { flex: 1; }

  .badge {
    font-size: .72rem; padding: .12rem .55rem; border-radius: 999px;
    border: 1px solid var(--border); color: var(--muted); white-space: nowrap;
  }
  .badge.good { color: var(--good); border-color: color-mix(in srgb, var(--good) 60%, transparent); }
  .badge.bad { color: var(--bad); border-color: color-mix(in srgb, var(--bad) 60%, transparent); }
  .badge.warn { color: var(--warn); border-color: color-mix(in srgb, var(--warn) 60%, transparent); }

  button {
    background: #222938; color: var(--text); border: 1px solid var(--border);
    border-radius: 7px; padding: .4rem .85rem; cursor: pointer; font: inherit;
    font-size: .85rem; transition: border-color .12s, background .12s;
  }
  button:hover { border-color: var(--accent); }
  button.primary { background: var(--accent2); border-color: var(--accent2); }
  button.primary:hover { filter: brightness(1.1); }
  button.subtle { background: transparent; color: var(--muted); }
  button.danger { color: var(--bad); }
  button.danger:hover { border-color: var(--bad); }
  .actions { display: flex; gap: .5rem; flex-wrap: wrap; margin-top: .8rem; }
  .actions .spacer { flex: 1; }

  .fields {
    display: grid; grid-template-columns: repeat(auto-fit, minmax(215px, 1fr));
    gap: .8rem; margin-top: .4rem;
  }
  .field label {
    display: block; font-size: .75rem; color: var(--muted);
    margin-bottom: .25rem; text-transform: uppercase; letter-spacing: .05em;
  }
  input, select {
    width: 100%; background: var(--bg); color: var(--text);
    border: 1px solid var(--border); border-radius: 7px; padding: .45rem .55rem;
    font: inherit; font-size: .88rem;
  }
  .chips { display: flex; flex-wrap: wrap; gap: .4rem; }
  .chip {
    display: inline-flex; align-items: center; gap: .35rem;
    border: 1px solid var(--border); border-radius: 999px;
    padding: .2rem .7rem; cursor: pointer; font-size: .82rem; color: var(--muted);
    user-select: none;
  }
  .chip input { display: none; }
  .chip.on { color: var(--text); border-color: var(--accent); background: #1b2740; }

  .newrow { display: flex; gap: .5rem; margin-top: .9rem; align-items: center; }
  .newrow input { max-width: 240px; }
  .token-box {
    background: var(--bg); border: 1px dashed var(--warn); border-radius: 8px;
    padding: .8rem .9rem; margin-top: .8rem; word-break: break-all;
    font-family: ui-monospace, monospace; font-size: .85rem;
  }
  .hint { color: var(--muted); font-size: .8rem; }
  .error { color: var(--bad); margin: .5rem 0; }
  #toast {
    position: fixed; bottom: 1.1rem; left: 50%; transform: translateX(-50%);
    background: var(--panel2); border: 1px solid var(--border);
    border-radius: 8px; padding: .55rem 1.1rem; display: none; z-index: 10;
  }
</style>
</head>
<body>
<div class="wrap">
  <header>
    <h1>Chatstronomy hub <span>· space | cat</span></h1>
    <span id="whoami"></span>
  </header>
  <div id="app"><p class="hint">Loading…</p></div>
</div>
<div id="toast"></div>
<script>
"use strict";
let CSRF = null;

const app = document.getElementById("app");
const esc = (s) => String(s ?? "").replace(/[&<>"']/g,
  (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));

function toast(msg) {
  const el = document.getElementById("toast");
  el.textContent = msg;
  el.style.display = "block";
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.style.display = "none"; }, 4000);
}

async function api(path, opts = {}) {
  opts.headers = Object.assign({}, opts.headers);
  if (opts.method && opts.method !== "GET") {
    opts.headers["x-csrf-token"] = CSRF;
    if (opts.body) opts.headers["content-type"] = "application/json";
  }
  const res = await fetch(path, opts);
  if (!res.ok) {
    throw new Error(await res.text() || res.statusText);
  }
  const type = res.headers.get("content-type") || "";
  return type.includes("json") ? res.json() : res.text();
}

async function boot() {
  const session = await api("/api/session");
  const who = document.getElementById("whoami");
  if (!session.authenticated) {
    who.innerHTML = "";
    app.innerHTML =
      '<div class="card"><h2>Bring your observatory into Discord</h2>' +
      '<p>Pair your N.I.N.A. rigs, route each telescope to a channel, and let your ' +
      "server watch sessions unfold — images, autofocus runs, guiding graphs, and " +
      "slash commands to drive the mount.</p>" +
      '<p><a href="/login"><button class="primary">Log in with Discord</button></a></p></div>';
    return;
  }
  CSRF = session.csrf_token;
  const u = session.user;
  who.innerHTML = (u.avatar_url ? '<img class="avatar" src="' + esc(u.avatar_url) + '" alt=""> ' : "") +
    esc(u.username) + ' &nbsp;·&nbsp; <a href="/logout">log out</a>';
  await renderGuilds();
}

async function renderGuilds() {
  app.innerHTML = '<p class="hint">Loading your servers…</p>';
  let data;
  try { data = await api("/api/guilds"); }
  catch (e) { app.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }
  if (!data.guilds.length) {
    app.innerHTML = '<div class="card"><p>No servers where you hold <b>Manage Server</b>. ' +
      "Ask a server admin, or check your Discord permissions.</p></div>";
    return;
  }
  app.innerHTML = "";
  for (const g of data.guilds) {
    const card = document.createElement("div");
    card.className = "card";
    const botBadge = g.bot_installed === null ? "" :
      g.bot_installed ? '<span class="badge good">bot installed</span>'
                      : '<span class="badge bad">bot not installed</span>';
    const regBadge = g.registered ? '<span class="badge good">registered</span>'
                                  : '<span class="badge">not registered</span>';
    let actions = "";
    if (g.bot_installed === false && g.install_url) {
      actions += '<a href="' + esc(g.install_url) + '" target="_blank" rel="noopener">' +
        '<button class="primary">Add bot to server</button></a> ';
    }
    if (!g.registered && g.bot_installed !== false) {
      actions += '<button class="primary" data-register="' + esc(g.id) + '">Set up this server</button>';
    }
    card.innerHTML =
      '<div class="head"><h2>' + esc(g.name) + "</h2>" +
      regBadge + " " + botBadge + " " + actions + "</div>" +
      '<div class="telescopes" data-guild="' + esc(g.id) + '"></div>';
    app.appendChild(card);
    if (g.registered) renderTelescopes(g.id, card.querySelector(".telescopes"));
  }
  app.querySelectorAll("[data-register]").forEach((btn) => {
    btn.onclick = async () => {
      try {
        await api("/api/guilds/" + btn.dataset.register + "/register", { method: "POST" });
        toast("Server registered");
        renderGuilds();
      } catch (e) { toast(e.message); }
    };
  });
}

const POLICY_OPTIONS = [
  ["admins", "Server managers"],
  ["roles", "Managers + selected roles"],
  ["disabled", "Disabled"],
];

function channelSelect(options, current) {
  // Discord-sourced picker. A routed channel that no longer appears in the
  // listing (deleted, or the bot lost visibility) stays selectable so the
  // form round-trips faithfully.
  let html = '<select class="f-channel"><option value="">— no channel (not posting) —</option>';
  let seen = false;
  for (const c of options.channels) {
    const sel = c.id === current ? " selected" : "";
    if (sel) seen = true;
    html += '<option value="' + esc(c.id) + '"' + sel + ">#" + esc(c.name) + "</option>";
  }
  if (current && !seen) {
    html += '<option value="' + esc(current) + '" selected>unknown channel (' + esc(current) + ")</option>";
  }
  return html + "</select>";
}

function roleChips(options, selected) {
  if (!options.roles.length) {
    return '<span class="hint">No assignable roles found in this server.</span>';
  }
  return '<div class="chips">' + options.roles.map((r) => {
    const on = selected.includes(r.id);
    return '<label class="chip' + (on ? " on" : "") + '"><input type="checkbox" value="' +
      esc(r.id) + '"' + (on ? " checked" : "") + ">@" + esc(r.name) + "</label>";
  }).join("") + "</div>";
}

async function renderTelescopes(guildId, el) {
  let data, options;
  try {
    [data, options] = await Promise.all([
      api("/api/guilds/" + guildId + "/telescopes"),
      api("/api/guilds/" + guildId + "/options"),
    ]);
  } catch (e) { el.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; return; }

  let html = "";
  for (const t of data.telescopes) {
    const connected = t.connected
      ? '<span class="badge good">connected</span>'
      : '<span class="badge warn">waiting for rig</span>';
    const policySelect = '<select class="f-policy">' + POLICY_OPTIONS.map(([v, label]) =>
      '<option value="' + v + '"' + (t.write_policy === v ? " selected" : "") + ">" +
      label + "</option>").join("") + "</select>";
    html +=
      '<div class="sub telescope" data-id="' + t.id + '">' +
      '<div class="head"><b>' + esc(t.name) + "</b>" + connected + '<span class="grow"></span></div>' +
      '<div class="fields">' +
      '<div class="field"><label>Posts to channel</label>' + channelSelect(options, t.discord_channel_id) + "</div>" +
      '<div class="field"><label>Image cooldown (seconds)</label>' +
      '<input class="f-cooldown" type="number" min="0" max="86400" value="' + t.image_cooldown_seconds + '"></div>' +
      '<div class="field"><label>Who can send commands</label>' + policySelect + "</div>" +
      "</div>" +
      '<div class="field roles-field" style="margin-top:.7rem' +
      (t.write_policy === "roles" ? "" : ";display:none") + '">' +
      "<label>Roles allowed to send commands (managers always can)</label>" +
      roleChips(options, t.allowed_role_ids) + "</div>" +
      '<div class="actions">' +
      '<button class="primary b-save">Save</button>' +
      '<button class="b-token">Pair a rig…</button>' +
      '<span class="spacer"></span>' +
      '<button class="subtle danger b-revoke">Revoke access</button>' +
      '<button class="subtle danger b-delete">Delete</button>' +
      "</div></div>";
  }
  html +=
    '<div class="newrow">' +
    '<input class="new-name" placeholder="telescope name (e.g. c925)">' +
    '<button class="b-create">Add telescope</button>' +
    '<span class="hint">One per N.I.N.A. profile or relay agent.</span></div>' +
    '<div class="token-out"></div>';
  el.innerHTML = html;

  el.querySelector(".b-create").onclick = async () => {
    const name = el.querySelector(".new-name").value.trim();
    if (!name) return;
    try {
      await api("/api/guilds/" + guildId + "/telescopes",
        { method: "POST", body: JSON.stringify({ name }) });
      toast("Telescope added — pair a rig to bring it online");
      renderTelescopes(guildId, el);
    } catch (e) { toast(e.message); }
  };

  el.querySelectorAll(".telescope").forEach((row) => {
    const id = row.dataset.id;
    const rolesField = row.querySelector(".roles-field");
    row.querySelector(".f-policy").onchange = (ev) => {
      rolesField.style.display = ev.target.value === "roles" ? "" : "none";
    };
    row.querySelectorAll(".chip input").forEach((box) => {
      box.onchange = () => box.closest(".chip").classList.toggle("on", box.checked);
    });
    row.querySelector(".b-save").onclick = async () => {
      const channel = row.querySelector(".f-channel").value;
      const roles = [...row.querySelectorAll(".chip input:checked")].map((box) => box.value);
      const body = {
        discord_channel_id: channel === "" ? null : channel,
        image_cooldown_seconds: parseInt(row.querySelector(".f-cooldown").value, 10) || 0,
        write_policy: row.querySelector(".f-policy").value,
        allowed_role_ids: roles,
      };
      try {
        await api("/api/telescopes/" + id, { method: "PATCH", body: JSON.stringify(body) });
        toast("Saved");
        renderTelescopes(guildId, el);
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-delete").onclick = async () => {
      if (!confirm("Delete this telescope? Its credentials and tokens go with it.")) return;
      try {
        await api("/api/telescopes/" + id, { method: "DELETE" });
        toast("Deleted");
        renderTelescopes(guildId, el);
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-revoke").onclick = async () => {
      if (!confirm("Revoke this telescope's credentials? The rig is disconnected " +
        "and must re-pair with a new token.")) return;
      try {
        await api("/api/telescopes/" + id + "/credentials", { method: "DELETE" });
        await api("/api/telescopes/" + id + "/pairing-tokens", { method: "DELETE" });
        toast("Access revoked");
        renderTelescopes(guildId, el);
      } catch (e) { toast(e.message); }
    };
    row.querySelector(".b-token").onclick = async () => {
      try {
        const out = await api("/api/telescopes/" + id + "/pairing-token", { method: "POST" });
        const box = el.querySelector(".token-out");
        box.innerHTML =
          '<div class="token-box">Pairing token (shown once, valid ' +
          Math.round(out.expires_in_seconds / 60) + " minutes):<br><b>" + esc(out.token) +
          '</b><br><button class="b-copy">Copy</button> ' +
          '<span class="hint">Paste into the N.I.N.A. plugin or the relay config, ' +
          "then connect. Issuing a new token cancels this one.</span></div>";
        box.querySelector(".b-copy").onclick = () => {
          navigator.clipboard.writeText(out.token).then(() => toast("Copied"));
        };
      } catch (e) { toast(e.message); }
    };
  });
}

boot().catch((e) => { app.innerHTML = '<p class="error">' + esc(e.message) + "</p>"; });
</script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_has_the_expected_hooks() {
        // The page drives these endpoints; renaming one must fail a test.
        for needle in [
            "/api/session",
            "/api/guilds",
            "/options",
            "/pairing-token",
            "/login",
            "/logout",
            "x-csrf-token",
        ] {
            assert!(INDEX_HTML.contains(needle), "missing {needle}");
        }
    }

    #[test]
    fn page_uses_pickers_not_id_inputs() {
        // Channels and roles are picked from Discord data, never typed.
        assert!(INDEX_HTML.contains("f-channel"));
        assert!(INDEX_HTML.contains("chips"));
        assert!(!INDEX_HTML.contains("placeholder=\"channel id\""));
        assert!(!INDEX_HTML.contains("placeholder=\"role ids\""));
    }

    #[test]
    fn page_offers_all_write_policies() {
        for policy in ["admins", "roles", "disabled"] {
            assert!(INDEX_HTML.contains(policy), "missing policy {policy}");
        }
    }
}
